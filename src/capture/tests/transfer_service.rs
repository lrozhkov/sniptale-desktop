use super::*;
use crate::capture::transfer_store::{
    ScreenshotStartupRecoverySummary, ScreenshotTransferStoreError,
};

#[test]
fn screenshot_capture_service_writes_transfer_and_messages() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open_with_default_config(temp.path())
        .map_err(|error| format!("{error:?}"))?;
    let mut service = ScreenshotCaptureService::new(
        store.clone(),
        FakeFrameProvider,
        FakePngWriter {
            bytes: b"hello world".to_vec(),
        },
    );
    let command = screenshot_command("command-1");

    let messages = service
        .capture_screenshot(&command)
        .map_err(|error| format!("{error:?}"))?;

    assert_eq!(messages.len(), 1);
    let ScreenshotCaptureEvent::Completed(start) = &messages[0];
    assert_eq!(start.mode, ScreenshotMode::Screen);
    assert_eq!(start.total_bytes, 11);
    assert_eq!(start.sha256, sha256_hex(b"hello world"));
    assert!(
        store
            .screenshot_media_path(&start.capture_id)
            .is_ok_and(|path| path.exists())
    );
    let media_path = store
        .screenshot_media_path(&start.capture_id)
        .map_err(|error| format!("{error:?}"))?;
    assert_ne!(read_test_file(&media_path)?, b"hello world");
    let metadata_path = temp.path().join(format!("{}.json", start.capture_id));
    let metadata = std::fs::read_to_string(metadata_path).map_err(|error| error.to_string())?;
    let metadata: serde_json::Value =
        serde_json::from_str(&metadata).map_err(|error| error.to_string())?;
    assert_eq!(metadata["routeSchema"], 1);
    assert!(metadata.get("authority").is_some());
    assert!(metadata.get("deliveryRoute").is_some());
    assert!(!metadata.to_string().contains("lease:connection-1"));

    let stored_chunk = store
        .screenshot_chunk_at(&start.capture_id, 0, command.requested_at_epoch_ms)
        .map_err(|error| format!("{error:?}"))?
        .ok_or("missing stored screenshot chunk")?;
    assert_eq!(stored_chunk.chunk.chunk_sha256, sha256_hex(b"hello world"));
    assert_eq!(stored_chunk.chunk.base64, "aGVsbG8gd29ybGQ=");
    assert!(stored_chunk.is_complete);
    assert!(matches!(
        store.screenshot_chunk_at(&start.capture_id, 0, command.requested_at_epoch_ms),
        Err(ScreenshotTransferStoreError::Transfer(
            TransferError::OutOfOrderChunk {
                expected: 1,
                actual: 0
            }
        ))
    ));

    Ok(())
}

#[test]
fn screenshot_finalize_chunk_read_path_handles_bounded_multi_chunk_media() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open_with_default_config(temp.path())
        .map_err(|error| format!("{error:?}"))?;
    let raw_limit =
        usize::try_from(crate::protocol::CHUNK_RAW_BYTES).map_err(|error| error.to_string())?;
    let payload = deterministic_payload(raw_limit + 17);
    let mut service = ScreenshotCaptureService::new(
        store.clone(),
        FakeFrameProvider,
        FakePngWriter {
            bytes: payload.clone(),
        },
    );
    let command = screenshot_command("command-multichunk");
    let messages = service
        .capture_screenshot(&command)
        .map_err(|error| format!("{error:?}"))?;
    let ScreenshotCaptureEvent::Completed(start) = &messages[0];

    assert_eq!(start.total_bytes, crate::protocol::CHUNK_RAW_BYTES + 17);
    assert_eq!(start.chunk_count, 2);
    let first = store
        .screenshot_chunk_at(&start.capture_id, 0, command.requested_at_epoch_ms)
        .map_err(|error| format!("{error:?}"))?
        .ok_or("missing first chunk")?;
    let second = store
        .screenshot_chunk_at(&start.capture_id, 1, command.requested_at_epoch_ms)
        .map_err(|error| format!("{error:?}"))?
        .ok_or("missing second chunk")?;

    assert_eq!(first.chunk.raw_bytes, crate::protocol::CHUNK_RAW_BYTES);
    assert!(!first.is_complete);
    assert_eq!(first.chunk.chunk_sha256, sha256_hex(&payload[..raw_limit]));
    assert_eq!(second.chunk.raw_bytes, 17);
    assert!(second.is_complete);
    assert_eq!(second.chunk.chunk_sha256, sha256_hex(&payload[raw_limit..]));
    Ok(())
}

#[test]
fn screenshot_transfer_store_rejects_tampered_protected_media() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open_with_default_config(temp.path())
        .map_err(|error| format!("{error:?}"))?;
    let mut service = ScreenshotCaptureService::new(
        store.clone(),
        FakeFrameProvider,
        FakePngWriter {
            bytes: b"hello world".to_vec(),
        },
    );
    let command = screenshot_command("command-tamper");
    let messages = service
        .capture_screenshot(&command)
        .map_err(|error| format!("{error:?}"))?;
    let ScreenshotCaptureEvent::Completed(start) = &messages[0];
    let media_path = store
        .screenshot_media_path(&start.capture_id)
        .map_err(|error| format!("{error:?}"))?;
    tamper_last_byte(&media_path)?;

    assert!(matches!(
        store.screenshot_chunk_at(&start.capture_id, 0, command.requested_at_epoch_ms),
        Err(ScreenshotTransferStoreError::Transfer(
            TransferError::ProtectedMedia
        ))
    ));
    Ok(())
}

#[test]
fn screenshot_transfer_store_rejects_metadata_downgraded_protected_media() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open_with_default_config(temp.path())
        .map_err(|error| format!("{error:?}"))?;
    let mut service = ScreenshotCaptureService::new(
        store.clone(),
        FakeFrameProvider,
        FakePngWriter {
            bytes: b"hello world".to_vec(),
        },
    );
    let command = screenshot_command("command-downgrade");
    let messages = service
        .capture_screenshot(&command)
        .map_err(|error| format!("{error:?}"))?;
    let ScreenshotCaptureEvent::Completed(start) = &messages[0];
    write_legacy_finalized_screenshot_metadata(temp.path(), &start.capture_id, 2_000)?;

    assert!(matches!(
        store.screenshot_chunk_at(&start.capture_id, 0, command.requested_at_epoch_ms),
        Err(ScreenshotTransferStoreError::Transfer(
            TransferError::ProtectedMedia
        ))
    ));
    Ok(())
}

#[test]
fn screenshot_transfer_store_rejects_plaintext_media_without_protection_metadata()
-> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open_with_default_config(temp.path())
        .map_err(|error| format!("{error:?}"))?;
    write_pending_screenshot(&store, temp.path(), "capture-plaintext", 2_000)?;

    assert!(matches!(
        store.screenshot_chunk_at("capture-plaintext", 0, 3_000),
        Err(ScreenshotTransferStoreError::Transfer(
            TransferError::ProtectedMedia
        ))
    ));
    Ok(())
}

#[test]
fn screenshot_capture_service_passes_selected_region_to_frame_provider() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open_with_default_config(temp.path())
        .map_err(|error| format!("{error:?}"))?;
    let region = PhysicalRect {
        x: 10,
        y: 20,
        width: 30,
        height: 40,
    };
    let mut service = ScreenshotCaptureService::new_with_region_selector(
        store,
        RegionAwareFrameProvider,
        FakePngWriter {
            bytes: b"hello world".to_vec(),
        },
        FakeRegionSelector { region },
    );
    let mut command = screenshot_command("command-region");
    command.mode = ScreenshotMode::Region;

    let messages = service
        .capture_screenshot(&command)
        .map_err(|error| format!("{error:?}"))?;

    let ScreenshotCaptureEvent::Completed(start) = &messages[0];
    assert_eq!(start.mode, ScreenshotMode::Region);
    assert_eq!(start.width, 30);
    assert_eq!(start.height, 40);
    Ok(())
}

#[test]
fn screenshot_capture_service_fails_closed_without_region_selector() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open_with_default_config(temp.path())
        .map_err(|error| format!("{error:?}"))?;
    let mut service = ScreenshotCaptureService::new(
        store,
        FakeFrameProvider,
        FakePngWriter {
            bytes: b"hello world".to_vec(),
        },
    );
    let mut command = screenshot_command("command-region");
    command.mode = ScreenshotMode::Region;

    assert_eq!(
        service.capture_screenshot(&command),
        Err(ScreenshotCaptureError::UnsupportedMode)
    );
    Ok(())
}

#[test]
fn screenshot_capture_service_persists_manual_route_before_finalizing_media() -> Result<(), String>
{
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open_with_default_config(temp.path())
        .map_err(|error| format!("{error:?}"))?;
    let mut service = ScreenshotCaptureService::new(
        store,
        FakeFrameProvider,
        FakePngWriter {
            bytes: b"hello world".to_vec(),
        },
    );
    let mut request = screenshot_command("manual-command");
    request.authority = OperationAuthority::Manual {
        profile_revision: "profile-revision-1".to_owned(),
    };
    request.delivery_route = DeliveryRoute::LocalAsset {
        asset_job_id: "asset-job-1".to_owned(),
    };

    assert!(service.capture_screenshot(&request).is_ok());
    assert!(
        std::fs::read_dir(temp.path())
            .map_err(|error| error.to_string())?
            .count()
            > 0
    );
    Ok(())
}

#[test]
fn screenshot_transfer_store_persists_prepared_extension_route() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open_with_default_config(temp.path())
        .map_err(|error| format!("{error:?}"))?;
    let request = screenshot_command("prepared-command");
    store
        .start_screenshot_at(
            super::super::transfer_record::PreparedScreenshotTransfer {
                capture_id: "capture-prepared".to_owned(),
                mode: request.mode,
                filename: "capture-prepared.png".to_owned(),
                bounds: PhysicalRect {
                    x: 0,
                    y: 0,
                    width: 10,
                    height: 10,
                },
                authority: request.authority,
                delivery_route: request.delivery_route,
                open_editor: request.open_editor,
            },
            request.requested_at_epoch_ms,
        )
        .map_err(|error| format!("{error:?}"))?;

    let metadata = std::fs::read_to_string(temp.path().join("capture-prepared.json"))
        .map_err(|error| error.to_string())?;
    assert!(metadata.contains("extension-transfer"));
    assert!(!metadata.contains("lease"));
    Ok(())
}

#[test]
fn screenshot_transfer_store_rejects_finalization_with_a_changed_route() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open_with_default_config(temp.path())
        .map_err(|error| format!("{error:?}"))?;
    let request = screenshot_command("prepared-command");
    let bounds = PhysicalRect {
        x: 0,
        y: 0,
        width: 10,
        height: 10,
    };
    store
        .start_screenshot_at(
            super::super::transfer_record::PreparedScreenshotTransfer {
                capture_id: "capture-prepared".to_owned(),
                mode: request.mode.clone(),
                filename: "capture-prepared.png".to_owned(),
                bounds,
                authority: request.authority.clone(),
                delivery_route: request.delivery_route.clone(),
                open_editor: request.open_editor,
            },
            request.requested_at_epoch_ms,
        )
        .map_err(|error| format!("{error:?}"))?;

    assert_eq!(
        store.finalize_prepared_screenshot_at(
            FinalizedScreenshotTransfer {
                capture_id: "capture-prepared".to_owned(),
                mode: request.mode,
                filename: "capture-prepared.png".to_owned(),
                bounds,
                authority: OperationAuthority::Manual {
                    profile_revision: "profile-revision-1".to_owned(),
                },
                delivery_route: DeliveryRoute::LocalAsset {
                    asset_job_id: "asset-job-1".to_owned(),
                },
                total_bytes: 11,
                sha256: sha256_hex(b"hello world"),
                media_protection: None,
                open_editor: request.open_editor,
            },
            request.requested_at_epoch_ms,
        ),
        Err(ScreenshotTransferStoreError::InvalidMetadata)
    );
    Ok(())
}

#[test]
fn screenshot_capture_service_removes_media_when_finalization_fails() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open_with_default_config(temp.path())
        .map_err(|error| format!("{error:?}"))?;
    let mut service = ScreenshotCaptureService::new(
        store.clone(),
        FakeFrameProvider,
        FakePngWriter { bytes: Vec::new() },
    );
    let command = screenshot_command("command-1");

    assert!(
        service
            .capture_screenshot(&command)
            .is_err_and(|error| matches!(
                error,
                ScreenshotCaptureError::TransferState | ScreenshotCaptureError::FileIo
            ))
    );

    let leaked = std::fs::read_dir(temp.path())
        .map_err(|error| error.to_string())?
        .count();
    assert_eq!(leaked, 0);
    Ok(())
}

#[test]
fn screenshot_transfer_store_ack_deletes_record_and_media() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open_with_default_config(temp.path())
        .map_err(|error| format!("{error:?}"))?;
    write_pending_screenshot(&store, temp.path(), "capture-1", 2_000)?;

    assert!(
        store
            .ack_screenshot_at("capture-1", 3_000)
            .map_err(|error| format!("{error:?}"))?
    );

    assert!(!temp.path().join("capture-1.json").exists());
    assert!(!temp.path().join("capture-1.png").exists());
    Ok(())
}

#[test]
fn screenshot_transfer_store_reject_retains_diagnostics_when_requested() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open_with_default_config(temp.path())
        .map_err(|error| format!("{error:?}"))?;
    write_pending_screenshot(&store, temp.path(), "capture-1", 2_000)?;

    assert!(
        store
            .reject_screenshot_at("capture-1", true, 3_000)
            .map_err(|error| format!("{error:?}"))?
    );

    assert!(temp.path().join("capture-1.json").exists());
    assert!(temp.path().join("capture-1.png").exists());
    Ok(())
}

#[test]
fn screenshot_transfer_store_reject_retains_protected_diagnostics() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open_with_default_config(temp.path())
        .map_err(|error| format!("{error:?}"))?;
    let mut service = ScreenshotCaptureService::new(
        store.clone(),
        FakeFrameProvider,
        FakePngWriter {
            bytes: b"hello world".to_vec(),
        },
    );
    let command = screenshot_command("command-reject-protected");
    let messages = service
        .capture_screenshot(&command)
        .map_err(|error| format!("{error:?}"))?;
    let ScreenshotCaptureEvent::Completed(start) = &messages[0];

    assert!(
        store
            .reject_screenshot_at(&start.capture_id, true, 5_000)
            .map_err(|error| format!("{error:?}"))?
    );
    let media_path = store
        .screenshot_media_path(&start.capture_id)
        .map_err(|error| format!("{error:?}"))?;
    assert_ne!(read_test_file(&media_path)?, b"hello world");
    Ok(())
}

#[test]
fn screenshot_transfer_store_summarizes_startup_recovery_scan() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open(
        temp.path(),
        ScreenshotTransferStoreConfig {
            writing_expiry: 500,
            finalized_expiry: 60_000,
            rejected_retention: 60_000,
        },
    )
    .map_err(|error| format!("{error:?}"))?;
    write_pending_screenshot(&store, temp.path(), "capture-1", 2_000)?;
    write_writing_screenshot(temp.path(), "capture-2", 2_800)?;
    write_writing_screenshot(temp.path(), "capture-3", 1_000)?;
    std::fs::write(temp.path().join("capture-3.png"), b"incomplete")
        .map_err(|error| error.to_string())?;

    let summary = store
        .startup_recovery_summary_at(3_000)
        .map_err(|error| format!("{error:?}"))?;

    assert_eq!(summary.pending_screenshots, 1);
    assert_eq!(summary.keep_writing, 0);
    assert_eq!(summary.deleted, 2);
    assert!(temp.path().join("capture-1.json").exists());
    assert!(!temp.path().join("capture-2.json").exists());
    assert!(!temp.path().join("capture-3.json").exists());
    assert!(!temp.path().join("capture-3.png").exists());
    Ok(())
}

#[test]
fn screenshot_transfer_store_lists_pending_screenshots_for_reconnect() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open(
        temp.path(),
        ScreenshotTransferStoreConfig {
            writing_expiry: 500,
            finalized_expiry: 60_000,
            rejected_retention: 60_000,
        },
    )
    .map_err(|error| format!("{error:?}"))?;
    write_pending_screenshot(&store, temp.path(), "capture-1", 2_000)?;
    write_writing_screenshot(temp.path(), "capture-2", 2_800)?;

    let pending = store
        .pending_screenshots_for_reconnect_at(3_000)
        .map_err(|error| format!("{error:?}"))?;

    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].capture_id, "capture-1");
    assert_eq!(pending[0].mode, ScreenshotMode::Screen);
    assert_eq!(pending[0].filename, "capture-1.png");
    assert_eq!(pending[0].total_bytes, 11);
    assert_eq!(pending[0].width, 10);
    assert_eq!(pending[0].height, 10);
    assert_eq!(pending[0].sha256, sha256_hex(b"hello world"));
    assert_eq!(pending[0].chunk_count, 1);
    assert!(pending[0].open_editor);
    assert_eq!(pending[0].captured_at_epoch_ms, 2_000);
    Ok(())
}

#[test]
fn screenshot_transfer_store_startup_recovery_summary_handles_empty_store() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open_with_default_config(temp.path())
        .map_err(|error| format!("{error:?}"))?;

    let summary = store
        .startup_recovery_summary()
        .map_err(|error| format!("{error:?}"))?;

    assert_eq!(summary, ScreenshotStartupRecoverySummary::default());
    Ok(())
}

#[test]
fn screenshot_transfer_store_periodic_cleanup_deletes_expired_records() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open(
        temp.path(),
        ScreenshotTransferStoreConfig {
            writing_expiry: 500,
            finalized_expiry: 60_000,
            rejected_retention: 60_000,
        },
    )
    .map_err(|error| format!("{error:?}"))?;
    write_pending_screenshot(&store, temp.path(), "capture-1", 2_000)?;
    write_writing_screenshot(temp.path(), "capture-2", 1_000)?;
    std::fs::write(temp.path().join("capture-2.png"), b"incomplete")
        .map_err(|error| error.to_string())?;

    let summary = store
        .cleanup_expired_at(3_000)
        .map_err(|error| format!("{error:?}"))?;

    assert_eq!(summary.pending_screenshots, 1);
    assert_eq!(summary.deleted, 1);
    assert!(temp.path().join("capture-1.json").exists());
    assert!(temp.path().join("capture-1.png").exists());
    assert!(!temp.path().join("capture-2.json").exists());
    assert!(!temp.path().join("capture-2.png").exists());
    Ok(())
}

#[test]
fn screenshot_transfer_cleanup_scan_classifies_bounded_mixed_records() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open(
        temp.path(),
        ScreenshotTransferStoreConfig {
            writing_expiry: 500,
            finalized_expiry: 500,
            rejected_retention: 500,
        },
    )
    .map_err(|error| format!("{error:?}"))?;
    write_pending_screenshot(&store, temp.path(), "capture-pending", 2_800)?;
    write_pending_screenshot(&store, temp.path(), "capture-expired", 1_000)?;
    write_pending_screenshot(&store, temp.path(), "capture-rejected", 2_500)?;
    assert!(
        store
            .reject_screenshot_at("capture-rejected", true, 2_700)
            .map_err(|error| format!("{error:?}"))?
    );
    write_writing_screenshot(temp.path(), "capture-writing", 2_800)?;

    let summary = store
        .cleanup_expired_at(3_000)
        .map_err(|error| format!("{error:?}"))?;

    assert_eq!(summary.pending_screenshots, 1);
    assert_eq!(summary.keep_writing, 1);
    assert_eq!(summary.retained_rejected, 1);
    assert_eq!(summary.deleted, 1);
    assert!(temp.path().join("capture-pending.json").exists());
    assert!(!temp.path().join("capture-expired.json").exists());
    assert!(temp.path().join("capture-rejected.json").exists());
    assert!(temp.path().join("capture-writing.json").exists());
    Ok(())
}

#[test]
fn screenshot_transfer_messages_stream_start_chunks_then_commit() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let path = temp.path().join("capture-1.png");
    std::fs::write(&path, b"hello world").map_err(|error| error.to_string())?;
    let session = screenshot_transfer_session(ScreenshotTransferSessionInput {
        controller_lease_id: "lease:connection-1".to_owned(),
        capture_id: "capture-1".to_owned(),
        mode: ScreenshotMode::Screen,
        filename: "capture-1.png".to_owned(),
        bounds: PhysicalRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        },
        total_bytes: 11,
        sha256: sha256_hex(b"hello world"),
        open_editor: true,
        captured_at_epoch_ms: 4_000,
    })
    .map_err(|error| format!("{error:?}"))?;

    let start = screenshot_start_message(&session);
    let chunk =
        screenshot_chunk_message(&path, &session, 0).map_err(|error| format!("{error:?}"))?;
    let commit = screenshot_commit_message(&session);

    assert_eq!(start.chunk_count, 1);
    assert_eq!(
        start.mime_type,
        crate::protocol::ScreenshotMimeType::ImagePng
    );
    assert_eq!(chunk.capture_id, "capture-1");
    assert_eq!(chunk.chunk_byte_offset, 0);
    assert_eq!(chunk.chunk_raw_bytes, 11);
    assert_eq!(chunk.chunk_sha256, sha256_hex(b"hello world"));
    assert_eq!(chunk.base64, "aGVsbG8gd29ybGQ=");
    assert_eq!(commit.capture_id, "capture-1");

    for message in [
        WireMessage::AppScreenshotStart(Box::new(start)),
        WireMessage::AppScreenshotChunk(Box::new(chunk)),
        WireMessage::AppScreenshotCommit(Box::new(commit)),
    ] {
        parse_wire_message(&serde_json::to_vec(&message).map_err(|error| error.to_string())?)
            .map_err(|error| format!("{error:?}"))?;
    }
    Ok(())
}

#[test]
fn screenshot_transfer_message_rejects_out_of_bounds_chunk() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let path = temp.path().join("capture-1.png");
    std::fs::write(&path, b"hello world").map_err(|error| error.to_string())?;
    let session = screenshot_transfer_session(ScreenshotTransferSessionInput {
        controller_lease_id: "lease:connection-1".to_owned(),
        capture_id: "capture-1".to_owned(),
        mode: ScreenshotMode::Screen,
        filename: "capture-1.png".to_owned(),
        bounds: PhysicalRect {
            x: 0,
            y: 0,
            width: 10,
            height: 10,
        },
        total_bytes: 11,
        sha256: sha256_hex(b"hello world"),
        open_editor: true,
        captured_at_epoch_ms: 4_000,
    })
    .map_err(|error| format!("{error:?}"))?;

    assert_eq!(
        screenshot_chunk_message(&path, &session, 1),
        Err(ScreenshotTransferMessageError::Transfer(
            TransferError::ChunkIndexOutOfBounds {
                index: 1,
                chunk_count: 1
            }
        ))
    );
    Ok(())
}

fn deterministic_payload(len: usize) -> Vec<u8> {
    (0..len)
        .map(|index| u8::try_from(index % 251).unwrap_or(0))
        .collect()
}
