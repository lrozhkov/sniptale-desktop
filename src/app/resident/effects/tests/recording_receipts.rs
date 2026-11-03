use super::*;
use crate::{
    protocol::{
        recording_events::{EventReceipt, ExtensionRecordingEventsAckMessage},
        trajectory::{
            CursorTrajectoryBlock, CursorTrajectoryReceipt, CursorTrajectoryRecord,
            ExtensionRecordingAttachmentAckMessage, encode_trajectory_block,
        },
    },
    transfer::{
        browser_delivery::{BrowserDeliveryStore, BrowserMediaKind},
        events_store::EventsWriter,
        trajectory_store::TrajectoryWriter,
    },
};

type ProofError = Box<dyn std::error::Error>;
const RECORDING: &str = "recording-receipts-proof";
const CURSOR: &str = "0123456789abcdef0123456789abcdef";
const EVENTS: &str = "abcdef0123456789abcdef0123456789";

struct Fixture {
    root: tempfile::TempDir,
    effects: AgentSideEffects,
    assignments: BrowserDeliveryStore,
    cursor: CursorTrajectoryReceipt,
    events: EventReceipt,
}

#[expect(
    clippy::too_many_lines,
    reason = "Builds one real video/cursor/events transaction fixture shared by all receipt-order and authorization proofs."
)]
fn fixture() -> Result<Fixture, ProofError> {
    let root = tempfile::tempdir()?;
    let target = "a".repeat(64);
    let route = DeliveryRoute::DesktopDelivery {
        asset_job_id: "receipts-job".to_owned(),
        target_id: Some(target.clone()),
        ask_after_capture: false,
    };
    let assignments = BrowserDeliveryStore::open(root.path().join("assignments"));
    assignments
        .select(RECORDING, BrowserMediaKind::Recording, &route, &target)
        .map_err(|error| format!("{error:?}"))?;
    let cursor_fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../contracts/native/fixtures/cursor-trajectory-v1.json"
    ))?;
    let mut block: CursorTrajectoryBlock =
        serde_json::from_value(cursor_fixture["canonicalVectors"][0]["value"].clone())?;
    block.recording_id = RECORDING.to_owned();
    block.end_time_us = 900_000;
    if let Some(CursorTrajectoryRecord::Boundary(boundary)) = block.records.last_mut() {
        boundary.active_time_us = 900_000;
    }
    let mut cursor = TrajectoryWriter::create(
        &root.path().join("cursor-trajectories"),
        RECORDING,
        CURSOR,
        route.clone(),
    )?;
    cursor.append(&encode_trajectory_block(&block).map_err(|error| format!("{error:?}"))?)?;
    let cursor_descriptor = cursor.finalize(false)?;
    drop(cursor);
    let event_fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../contracts/native/fixtures/recording-events-v1.json"
    ))?;
    let mut events = EventsWriter::create(
        &root.path().join("recording-events"),
        RECORDING,
        EVENTS,
        route.clone(),
    )?;
    for source in event_fixture["blocks"].as_array().ok_or("blocks")? {
        let mut block = source["value"].clone();
        block["recordingId"] = serde_json::json!(RECORDING);
        events.append(&serde_json::to_vec(&block)?)?;
    }
    let event_descriptor = events.finalize(false)?;
    drop(events);
    let store = RecordingTransferStore::open_with_default_config(root.path())
        .map_err(|error| format!("{error:?}"))?;
    let now = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
    let path = store
        .start_recording_with_route_at(
            RECORDING,
            OperationAuthority::Manual {
                profile_revision: "profile-proof".to_owned(),
            },
            route,
            now,
        )
        .map_err(|error| format!("{error:?}"))?;
    std::fs::write(path, b"durable-video-proof")?;
    store
        .finalize_manual_recording_with_telemetry_at(
            RECORDING,
            crate::recording::RecordingMediaMetadata {
                filename: format!("{RECORDING}.mp4"),
                duration_ms: 900,
                width: 1280,
                height: 800,
                fps: 30,
                open_editor: false,
                cursor_trajectory: Some(cursor_descriptor.clone()),
                recording_events: Some(event_descriptor.clone()),
            },
            now,
            None,
        )
        .map_err(|error| format!("{error:?}"))?;
    let mut effects = AgentSideEffects::new(
        HotkeyWorkerPlanSink::new(None),
        TrayWorkerRegistrySink::new(None),
        None,
        CaptureWorkerCommandSink::new(None),
        Some(store),
        RecordingWorkerCommandSink::new(None),
    );
    effects.browser_target_id = Some(target);
    effects.assignments = Some(assignments.clone());
    let cursor = CursorTrajectoryReceipt {
        version: 1,
        durable: true,
        format: cursor_descriptor.format,
        state: cursor_descriptor.state,
        total_blocks: cursor_descriptor.total_blocks,
        total_records: cursor_descriptor.total_records,
        total_samples: cursor_descriptor.total_samples,
        total_bytes: cursor_descriptor.total_bytes,
        stream_sha256: cursor_descriptor.stream_sha256,
    };
    let events = EventReceipt {
        version: 1,
        durable: true,
        format: event_descriptor.format,
        state: event_descriptor.state,
        total_blocks: event_descriptor.total_blocks,
        total_records: event_descriptor.total_records,
        total_bytes: event_descriptor.total_bytes,
        stream_sha256: event_descriptor.stream_sha256,
    };
    Ok(Fixture {
        root,
        effects,
        assignments,
        cursor,
        events,
    })
}

fn cursor_ack(receipt: CursorTrajectoryReceipt) -> ExtensionRecordingAttachmentAckMessage {
    ExtensionRecordingAttachmentAckMessage {
        protocol_version: 2,
        controller_lease_id: "lease-receipts".to_owned(),
        recording_id: RECORDING.to_owned(),
        trajectory_id: CURSOR.to_owned(),
        request_id: "cursor-ack".to_owned(),
        receipt,
    }
}
fn event_ack(receipt: EventReceipt) -> ExtensionRecordingEventsAckMessage {
    ExtensionRecordingEventsAckMessage {
        protocol_version: 2,
        controller_lease_id: "lease-receipts".to_owned(),
        recording_id: RECORDING.to_owned(),
        event_stream_id: EVENTS.to_owned(),
        request_id: "events-ack".to_owned(),
        receipt,
    }
}

#[test]
fn every_receipt_order_preserves_unacknowledged_history_after_video_cleanup()
-> Result<(), ProofError> {
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let mut fixture = fixture()?;
        for (step, receipt) in order.into_iter().enumerate() {
            let completed = match receipt {
                0 => fixture.effects.ack_recording_transfer_in_store(RECORDING),
                1 => fixture
                    .effects
                    .ack_recording_attachment_in_store(&cursor_ack(fixture.cursor.clone())),
                _ => fixture
                    .effects
                    .ack_recording_events_in_store(&event_ack(fixture.events.clone())),
            }
            .map_err(|error| format!("{error:?}"))?;
            assert!(completed);
            let selected = fixture
                .assignments
                .target(RECORDING, BrowserMediaKind::Recording)
                .map_err(|error| format!("{error:?}"))?;
            assert_eq!(selected.is_some(), step != 2, "{order:?} step={step}");
            let store = fixture.effects.recordings.as_ref().ok_or("store")?;
            assert_eq!(
                store
                    .cursor_trajectory_catalog()
                    .lookup(RECORDING, CURSOR)?
                    .is_some(),
                step != 2
            );
            assert_eq!(
                store
                    .recording_events_catalog()
                    .lookup(RECORDING, EVENTS)?
                    .is_some(),
                step != 2
            );
            if receipt == 0 {
                assert!(
                    !fixture
                        .root
                        .path()
                        .join(format!("{RECORDING}.mp4"))
                        .exists()
                );
            }
        }
        // Retransmitted receipts after complete cleanup do not reopen or recreate sources.
        assert!(
            !fixture
                .effects
                .ack_recording_attachment_in_store(&cursor_ack(fixture.cursor.clone()))
                .map_err(|error| format!("{error:?}"))?
        );
        assert!(
            !fixture
                .effects
                .ack_recording_events_in_store(&event_ack(fixture.events.clone()))
                .map_err(|error| format!("{error:?}"))?
        );
    }
    Ok(())
}

#[test]
fn busy_history_refuses_video_receipt_without_losing_media_and_allows_retry()
-> Result<(), ProofError> {
    for hold_cursor in [true, false] {
        let mut fixture = fixture()?;
        let before = fixture
            .effects
            .pending_recordings_for_reconnect_from_store()
            .map_err(|error| format!("{error:?}"))?;
        let cursor_lock = hold_cursor
            .then(|| {
                crate::transfer::trajectory_store::TrajectoryArchive::open(
                    &fixture.root.path().join("cursor-trajectories"),
                    RECORDING,
                    CURSOR,
                )
            })
            .transpose()?;
        let event_lock = (!hold_cursor)
            .then(|| {
                crate::transfer::events_store::EventsArchive::open(
                    &fixture.root.path().join("recording-events"),
                    RECORDING,
                    EVENTS,
                )
            })
            .transpose()?;
        assert!(
            fixture
                .effects
                .ack_recording_transfer_in_store(RECORDING)
                .is_err()
        );
        assert_eq!(
            fixture
                .effects
                .pending_recordings_for_reconnect_from_store()
                .map_err(|error| format!("{error:?}"))?,
            before
        );
        let now = u64::try_from(SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis())?;
        let chunk = fixture
            .effects
            .recording_transfer_chunk_from_store(RECORDING, 0, now)
            .map_err(|error| format!("{error:?}"))?
            .ok_or("video body was lost after refused receipt")?;
        let video = before.first().ok_or("missing original video metadata")?;
        assert_eq!(chunk.raw_bytes, video.total_bytes);
        assert_eq!(chunk.chunk_sha256, video.sha256);
        drop((cursor_lock, event_lock));
        assert!(
            fixture
                .effects
                .ack_recording_transfer_in_store(RECORDING)
                .map_err(|error| format!("{error:?}"))?
        );
        assert_eq!(
            fixture
                .effects
                .pending_recordings_for_reconnect_from_store()
                .map_err(|error| format!("{error:?}"))?,
            Vec::<crate::recording::PendingRecordingTransfer>::new()
        );
        let histories = fixture
            .assignments
            .recording_histories(RECORDING, &"a".repeat(64))
            .map_err(|error| format!("{error:?}"))?
            .ok_or("history assignment removed before history receipts")?;
        assert_eq!(histories.cursor_trajectory_id.as_deref(), Some(CURSOR));
        assert_eq!(histories.event_stream_id.as_deref(), Some(EVENTS));
    }
    Ok(())
}

#[test]
fn a_complete_receipt_cannot_erase_gap_history_and_another_browser_cannot_ack_it()
-> Result<(), ProofError> {
    let mut fixture = fixture()?;
    let mut forged = fixture.events.clone();
    forged.state = crate::protocol::trajectory::CursorTrajectoryState::Complete;
    assert!(
        fixture
            .effects
            .ack_recording_events_in_store(&event_ack(forged))
            .is_err()
    );
    fixture.effects.browser_target_id = Some("b".repeat(64));
    assert!(
        fixture
            .effects
            .ack_recording_events_in_store(&event_ack(fixture.events.clone()))
            .is_err()
    );
    let item = fixture
        .effects
        .recordings
        .as_ref()
        .ok_or("store")?
        .recording_events_catalog()
        .lookup(RECORDING, EVENTS)?
        .ok_or("history")?;
    assert!(!item.event_receipt_accepted);
    assert_eq!(
        item.descriptor.state,
        crate::protocol::trajectory::CursorTrajectoryState::Incomplete
    );
    assert_eq!(
        fixture
            .assignments
            .target(RECORDING, BrowserMediaKind::Recording)
            .map_err(|error| format!("{error:?}"))?,
        Some("a".repeat(64))
    );
    Ok(())
}
