use super::*;
use crate::protocol::DeliveryRoute;

#[test]
fn a_failed_writer_retires_its_input_before_recovery_and_late_commands_cannot_recreate_it()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let fixture: serde_json::Value = crate::protocol::recording_events::recording_events_fixture()?;
    let recording = fixture["recordingId"].as_str().ok_or("recording")?;
    let stream = fixture["eventStreamId"].as_str().ok_or("stream")?;
    let route = DeliveryRoute::DesktopDelivery {
        asset_job_id: "retirement-proof".to_owned(),
        target_id: Some("a".repeat(64)),
        ask_after_capture: false,
    };
    let slot = Arc::new(Mutex::new(WriterSlot::default()));
    let mut sink =
        spawn_writer::<EventsWriter>(root.path(), &slot, recording, stream, route.clone())
            .map_err(|error| format!("{error:?}"))?;
    let directory = std::fs::read_dir(root.path())?
        .next()
        .ok_or("source directory")??
        .path();
    std::fs::create_dir(directory.join("head.pending"))?;
    let bytes = fixture["blocks"][0]["utf8"]
        .as_str()
        .ok_or("bytes")?
        .as_bytes()
        .to_vec();
    sink.try_append(bytes.clone())
        .map_err(|error| format!("{error:?}"))?;
    {
        let state = sink.status.0.lock().map_err(|_| "writer status")?;
        let (state, _) = sink
            .status
            .1
            .wait_timeout_while(state, Duration::from_secs(15), |state| !state.done)
            .map_err(|_| "writer completion")?;
        assert!(state.done);
        assert_eq!(state.descriptor.total_blocks, 0);
    }
    assert_eq!(sink.control.load(Ordering::Acquire), FINALIZE_INCOMPLETE);
    assert_eq!(
        sink.try_append(bytes),
        Err(CursorTrajectorySinkError::Retired)
    );
    std::fs::remove_dir(directory.join("head.pending"))?;
    let erasure = sink.erase();
    drop(sink);
    let worker = slot
        .lock()
        .map_err(|_| "slot")?
        .parked
        .take()
        .ok_or("worker")?;
    worker.join().map_err(|_| "writer panic")?;
    if let Err(error) = erasure {
        assert_eq!(error, CursorTrajectorySinkError::Storage);
    }
    assert!(!directory.join("head.json").exists());
    assert!(EventsWriter::create(root.path(), recording, stream, route).is_err());
    assert!(!directory.join("blocks.bin").exists());
    Ok(())
}
