use super::*;
use crate::{
    protocol::{DeliveryRoute, trajectory::CursorTrajectoryState},
    telemetry::RecordingFrameGeometry,
    transfer::hashing::sha256_hex,
};

type ProofError = Box<dyn std::error::Error>;
const RECORDING: &str = "recording-fixture";
const TRAJECTORY: &str = "0123456789abcdef0123456789abcdef";

fn context() -> RecordingTrajectoryContext {
    RecordingTrajectoryContext {
        point_mapping: crate::telemetry::RecordingPointMapping::FixedDesktopFrame,
        frame: RecordingFrameGeometry {
            x: 0,
            y: 0,
            width: 1280,
            height: 800,
        },
        delivery_route: DeliveryRoute::DesktopDelivery {
            asset_job_id: "job-fixture".to_owned(),
            target_id: Some("a".repeat(64)),
            ask_after_capture: false,
        },
    }
}

fn fixture() -> Result<Vec<u8>, ProofError> {
    let fixture: serde_json::Value = crate::protocol::trajectory::trajectory_fixture()?;
    Ok(fixture["canonicalVectors"][0]["utf8"]
        .as_str()
        .ok_or("bytes")?
        .as_bytes()
        .to_vec())
}

fn factory(root: &std::path::Path) -> CursorWriterFactory {
    CursorWriterFactory {
        root: root.to_owned(),
        slot: Arc::new(Mutex::new(WriterSlot::default())),
    }
}

fn join_parked(factory: &CursorWriterFactory) -> Result<(), ProofError> {
    let worker = factory
        .slot
        .lock()
        .map_err(|_| "slot")?
        .parked
        .take()
        .ok_or("worker")?;
    worker.join().map_err(|_| "writer panic")?;
    Ok(())
}

#[test]
fn writer_finalizes_durable_prefix_and_keeps_one_process_owned_slot() -> Result<(), ProofError> {
    let root = tempfile::tempdir()?;
    let mut factory = factory(root.path());
    let mut sink = factory
        .create(RECORDING, TRAJECTORY, &context())
        .map_err(|error| format!("{error:?}"))?;
    assert!(
        factory
            .create("second-recording", &"b".repeat(32), &context())
            .is_err()
    );
    let bytes = fixture()?;
    sink.try_append(bytes.clone())
        .map_err(|error| format!("{error:?}"))?;
    let descriptor = sink.finish(false);
    assert_eq!(
        sink.try_append(bytes.clone()),
        Err(CursorTrajectorySinkError::Retired)
    );
    drop(sink);
    join_parked(&factory)?;
    let mut archive = TrajectoryArchive::open(root.path(), RECORDING, TRAJECTORY)?;
    assert_eq!(archive.read_block(0)?, bytes);
    assert!(archive.descriptor().finalized);
    assert_eq!(archive.descriptor().state, CursorTrajectoryState::Complete);
    assert_eq!(archive.descriptor().total_blocks, 1);
    if descriptor.finalized {
        assert_eq!(archive.descriptor(), &descriptor);
    } else {
        assert!(descriptor.total_blocks <= archive.descriptor().total_blocks);
    }
    Ok(())
}

#[test]
fn full_queue_is_nonblocking_and_requests_incomplete_finalization() -> Result<(), ProofError> {
    let root = tempfile::tempdir()?;
    let writer =
        TrajectoryWriter::create(root.path(), RECORDING, TRAJECTORY, context().delivery_route)?;
    let status = Arc::new((
        Mutex::new(WriterStatus {
            descriptor: writer.descriptor().clone(),
            done: false,
            erased: false,
        }),
        Condvar::new(),
    ));
    let (sender, _receiver) = mpsc::sync_channel(2);
    let control = Arc::new(AtomicU8::new(0));
    let mut sink = CursorWriterSink {
        sender,
        control: control.clone(),
        status,
        worker: None,
        slot: Arc::new(Mutex::new(WriterSlot::default())),
    };
    sink.try_append(vec![1])
        .map_err(|error| format!("{error:?}"))?;
    sink.try_append(vec![2])
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        sink.try_append(vec![3]),
        Err(CursorTrajectorySinkError::Backpressure)
    );
    assert_eq!(control.load(Ordering::Acquire), FINALIZE_INCOMPLETE);
    assert_eq!(
        sink.try_append(vec![4]),
        Err(CursorTrajectorySinkError::Retired)
    );
    let committed = writer.descriptor().clone();
    let timed_out = sink.finish(false);
    assert_eq!(timed_out, committed);
    assert!(!timed_out.finalized);
    assert!(!sink.status.0.lock().map_err(|_| "writer status")?.done);
    Ok(())
}

#[test]
fn privacy_erase_after_finish_is_processed_by_the_owned_writer() -> Result<(), ProofError> {
    let root = tempfile::tempdir()?;
    let mut factory = factory(root.path());
    let mut sink = factory
        .create(RECORDING, TRAJECTORY, &context())
        .map_err(|error| format!("{error:?}"))?;
    sink.try_append(fixture()?)
        .map_err(|error| format!("{error:?}"))?;
    let _committed_prefix = sink.finish(false);
    let erasure = sink.erase();
    drop(sink);
    join_parked(&factory)?;
    if let Err(error) = erasure {
        assert_eq!(error, CursorTrajectorySinkError::Storage);
    }
    assert!(TrajectoryArchive::open(root.path(), RECORDING, TRAJECTORY).is_err());
    let directory = root
        .path()
        .join(format!("{}-{TRAJECTORY}", sha256_hex(RECORDING.as_bytes())));
    assert!(!directory.join("blocks.bin").exists());
    assert!(!directory.join("head.json").exists());
    assert!(factory.create(RECORDING, TRAJECTORY, &context()).is_err());
    Ok(())
}

#[test]
fn runtime_source_writes_original_event_times_and_raw_button_edges() -> Result<(), ProofError> {
    use crate::{
        protocol::trajectory::{
            CursorTrajectoryEvent, CursorTrajectoryRecord, decode_trajectory_block,
        },
        telemetry::{
            RecordingTelemetryPolicy, RecordingTelemetrySource, RuntimeRecordingTelemetrySource,
            WindowsMouseButton, WindowsMouseEvent, WindowsMouseEventKind,
        },
    };
    let root = tempfile::tempdir()?;
    let factory = factory(root.path());
    let reaper = CursorWriterFactory {
        root: root.path().to_owned(),
        slot: factory.slot.clone(),
    };
    let mut source = RuntimeRecordingTelemetrySource::with_trajectory_factory(
        || 2_000_000,
        || 2000,
        Some(Box::new(factory)),
    );
    source.configure_trajectory(Some(context()));
    source.configure_recording(RecordingTelemetryPolicy::all_enabled_for_tests());
    source.start_recording(RECORDING, 1_000_000);
    for (sequence, kind) in [
        (0, WindowsMouseEventKind::Move),
        (1, WindowsMouseEventKind::Down(WindowsMouseButton::Left)),
        (2, WindowsMouseEventKind::Up(WindowsMouseButton::Left)),
    ] {
        source
            .inject_mouse_event(
                RECORDING,
                WindowsMouseEvent {
                    sequence,
                    source_time_ms: 2001 + sequence,
                    x: 50,
                    y: 60,
                    visible: Some(true),
                    kind,
                },
                99_000_000,
            )
            .map_err(|error| format!("{error:?}"))?;
    }
    source.finish_recording(RECORDING, 10_000_000);
    let mut snapshot = source
        .snapshot_for_recording(RECORDING, 9)
        .ok_or("snapshot")?;
    let descriptor = snapshot.take_cursor_trajectory().ok_or("descriptor")?;
    let legacy = snapshot.into_protocol();
    assert!(legacy.cursor_track.is_none());
    assert_eq!(legacy.action_events, Vec::<serde_json::Value>::new());
    join_parked(&reaper)?;
    let mut archive = TrajectoryArchive::open(root.path(), RECORDING, &descriptor.trajectory_id)?;
    assert!(archive.descriptor().finalized);
    assert_eq!(archive.descriptor().state, CursorTrajectoryState::Complete);
    assert_eq!(archive.descriptor().total_samples, 3);
    if descriptor.finalized {
        assert_eq!(archive.descriptor(), &descriptor);
    } else {
        assert!(descriptor.total_samples <= archive.descriptor().total_samples);
    }
    let block =
        decode_trajectory_block(&archive.read_block(0)?).map_err(|error| format!("{error:?}"))?;
    let samples: Vec<_> = block
        .records
        .iter()
        .filter_map(|record| {
            if let CursorTrajectoryRecord::Sample(sample) = record {
                Some(sample)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        samples
            .iter()
            .map(|sample| sample.active_time_us)
            .collect::<Vec<_>>(),
        [2000, 3000, 4000]
    );
    assert_eq!(
        samples
            .iter()
            .map(|sample| sample.event)
            .collect::<Vec<_>>(),
        [
            CursorTrajectoryEvent::Move,
            CursorTrajectoryEvent::Down,
            CursorTrajectoryEvent::Up
        ]
    );
    Ok(())
}

type CursorCollectionProof = (
    Option<crate::protocol::trajectory::CursorTrajectoryDescriptor>,
    Vec<crate::protocol::trajectory::CursorTrajectoryRecord>,
    bool,
);

fn collect_pointer_policy(cursor: bool, clicks: bool) -> Result<CursorCollectionProof, ProofError> {
    collect_pointer_policy_with_context(cursor, clicks, context())
}

fn collect_pointer_policy_with_context(
    cursor: bool,
    clicks: bool,
    context: RecordingTrajectoryContext,
) -> Result<CursorCollectionProof, ProofError> {
    use crate::telemetry::{
        RecordingTelemetryPolicy, RecordingTelemetrySource, RuntimeRecordingTelemetrySource,
        WindowsMouseButton, WindowsMouseEvent, WindowsMouseEventKind,
    };
    let root = tempfile::tempdir()?;
    let factory = factory(root.path());
    let slot = factory.slot.clone();
    let mut source = RuntimeRecordingTelemetrySource::with_trajectory_factory(
        || 0,
        || 1000,
        Some(Box::new(factory)),
    );
    source.configure_trajectory(Some(context));
    source.configure_recording(RecordingTelemetryPolicy {
        collect_cursor: cursor,
        collect_clicks: clicks,
        ..RecordingTelemetryPolicy::default()
    });
    source.start_recording(RECORDING, 0);
    for (index, kind) in [
        WindowsMouseEventKind::Move,
        WindowsMouseEventKind::Down(WindowsMouseButton::Left),
        WindowsMouseEventKind::Up(WindowsMouseButton::Left),
    ]
    .into_iter()
    .enumerate()
    {
        let sequence = u64::try_from(index)?;
        source
            .inject_mouse_event(
                RECORDING,
                WindowsMouseEvent {
                    sequence,
                    source_time_ms: 1001 + sequence,
                    visible: Some(true),
                    x: 32 + i32::try_from(index)? * 32,
                    y: 64,
                    kind,
                },
                (sequence + 1) * 1_000_000,
            )
            .map_err(|error| format!("{error:?}"))?;
    }
    source.finish_recording(RECORDING, 10_000_000);
    let descriptor = source
        .snapshot_for_recording(RECORDING, 10)
        .ok_or("snapshot")?
        .take_cursor_trajectory();
    let worker = slot.lock().map_err(|_| "slot")?.parked.take();
    if let Some(worker) = worker {
        worker.join().map_err(|_| "writer panic")?;
    }
    let mut records = Vec::new();
    if let Some(descriptor) = &descriptor
        && descriptor.state != CursorTrajectoryState::Unavailable
    {
        let mut archive =
            TrajectoryArchive::read(root.path(), RECORDING, &descriptor.trajectory_id)?;
        for sequence in 0..descriptor.total_blocks {
            records.extend(
                crate::protocol::trajectory::decode_trajectory_block(
                    &archive.read_block(sequence)?,
                )
                .map_err(|error| format!("{error:?}"))?
                .records,
            );
        }
    }
    Ok((
        descriptor,
        records,
        root.path().read_dir()?.next().is_some(),
    ))
}

#[test]
fn click_only_policy_never_allocates_or_exposes_a_cursor_source() -> Result<(), ProofError> {
    let (descriptor, records, allocated) = collect_pointer_policy(false, true)?;
    assert!(descriptor.is_none());
    assert_eq!(records.len(), 0);
    assert!(!allocated);
    Ok(())
}

#[test]
fn cursor_only_policy_retains_positions_without_button_semantics() -> Result<(), ProofError> {
    use crate::protocol::trajectory::{
        CursorTrajectoryButton, CursorTrajectoryEvent, CursorTrajectoryRecord,
    };
    let (descriptor, records, allocated) = collect_pointer_policy(true, false)?;
    assert!(descriptor.is_some());
    assert!(allocated);
    let samples: Vec<_> = records
        .iter()
        .filter_map(|record| match record {
            CursorTrajectoryRecord::Sample(sample) => Some(sample),
            _ => None,
        })
        .collect();
    assert!(samples.len() >= 2);
    assert!(
        samples
            .iter()
            .all(|sample| sample.event == CursorTrajectoryEvent::Move
                && sample.button == CursorTrajectoryButton::None)
    );
    Ok(())
}

#[test]
fn an_untracked_window_origin_never_certifies_a_cursor_source() -> Result<(), ProofError> {
    let mut context = context();
    context.point_mapping = crate::telemetry::RecordingPointMapping::for_capture_mode(
        &crate::protocol::VideoMode::ActiveWindow,
    );
    let (descriptor, records, allocated) =
        collect_pointer_policy_with_context(true, true, context)?;
    let descriptor = descriptor.ok_or("explicit cursor state")?;
    assert_eq!(descriptor.state, CursorTrajectoryState::Unavailable);
    assert!(descriptor.finalized);
    assert_eq!(descriptor.total_samples, 0);
    assert_eq!(descriptor.total_blocks, 0);
    assert_eq!(records.len(), 0);
    assert!(!allocated);
    Ok(())
}
