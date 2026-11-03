use super::*;
use crate::{
    protocol::{
        DeliveryRoute,
        recording_events::{EventRecord, decode_event_block},
        trajectory::CursorTrajectoryState,
    },
    telemetry::{
        RecordingFrameGeometry, RecordingTelemetryPolicy, RecordingTelemetrySource,
        RuntimeRecordingTelemetrySource, WindowsKeyEvent, WindowsKeyModifiers,
        WindowsKeyObservation, WindowsKeyPhase,
    },
    transfer::events_store::EventsArchive,
};

#[test]
fn runtime_source_persists_original_key_times_and_exposes_the_actual_durable_descriptor()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let slot = Arc::new(Mutex::new(WriterSlot::default()));
    let factory = EventsWriterFactory {
        root: root.path().to_owned(),
        slot: slot.clone(),
    };
    let mut source = RuntimeRecordingTelemetrySource::with_trajectory_factory(|| 0, || 1000, None)
        .with_events_factory(Box::new(factory));
    source.configure_trajectory(Some(RecordingTrajectoryContext {
        point_mapping: crate::telemetry::RecordingPointMapping::FixedDesktopFrame,
        frame: RecordingFrameGeometry {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
        },
        delivery_route: DeliveryRoute::DesktopDelivery {
            asset_job_id: "native-events-runtime-proof".to_owned(),
            target_id: Some("a".repeat(64)),
            ask_after_capture: false,
        },
    }));
    source.configure_recording(RecordingTelemetryPolicy::all_enabled_for_tests());
    source.start_recording("events-runtime-proof", 0);
    for (sequence, phase) in [
        WindowsKeyPhase::Down,
        WindowsKeyPhase::Repeat,
        WindowsKeyPhase::Up,
    ]
    .into_iter()
    .enumerate()
    {
        source
            .inject_key_observation(
                "events-runtime-proof",
                WindowsKeyObservation {
                    event: WindowsKeyEvent {
                        virtual_key: 0x74,
                        modifiers: WindowsKeyModifiers::default(),
                    },
                    sequence: u64::try_from(sequence)?,
                    source_time_ms: 1001 + u64::try_from(sequence)?,
                    phase,
                },
            )
            .map_err(|error| format!("{error:?}"))?;
    }
    source.finish_recording("events-runtime-proof", 1_000_000_000);
    let mut snapshot = source
        .snapshot_for_recording("events-runtime-proof", 1000)
        .ok_or("snapshot")?;
    let descriptor = snapshot
        .take_recording_events()
        .ok_or("events descriptor")?;
    let worker = slot
        .lock()
        .map_err(|_| "slot")?
        .parked
        .take()
        .ok_or("worker")?;
    worker.join().map_err(|_| "writer panic")?;
    let mut archive = EventsArchive::read(
        root.path(),
        "events-runtime-proof",
        &descriptor.event_stream_id,
    )?;
    assert!(archive.descriptor().finalized);
    assert_eq!(archive.descriptor().state, CursorTrajectoryState::Complete);
    if descriptor.finalized {
        assert_eq!(archive.descriptor(), &descriptor);
    } else {
        assert!(descriptor.total_records <= archive.descriptor().total_records);
    }
    let block =
        decode_event_block(&archive.read_block(0)?).map_err(|error| format!("{error:?}"))?;
    let update = block
        .records
        .iter()
        .find_map(|record| match record {
            EventRecord::ActionUpdate { data, .. } => Some(serde_json::to_value(data)),
            _ => None,
        })
        .ok_or("completion")??;
    assert_eq!(update["lastObservedTimeUs"], 3000);
    assert_eq!(update["upTimeUs"], 3000);
    assert_eq!(update["pressDurationUs"], 2000);
    assert_eq!(update["repeatCount"], 1);
    assert_eq!(
        snapshot.into_protocol().action_events,
        Vec::<serde_json::Value>::new()
    );
    Ok(())
}
