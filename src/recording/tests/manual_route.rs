use super::super::RecordingTransferStore;
use super::super::temp_media::RecordingMediaMetadata;
use super::super::temp_media::{TempMediaRecord, TempStoreError};
use super::super::transfer_store::RecordingTransferStoreConfig;
use crate::protocol::{DeliveryRoute, OperationAuthority};
use tempfile::tempdir;

#[test]
fn manual_recording_route_is_persisted_as_an_immutable_local_asset_snapshot()
-> Result<(), Box<dyn std::error::Error>> {
    let record = TempMediaRecord::start_writing_with_route(
        "recording-1",
        OperationAuthority::Manual {
            profile_revision: "profile-7".to_owned(),
        },
        DeliveryRoute::LocalAsset {
            asset_job_id: "asset-job-7".to_owned(),
        },
        1_000,
    )
    .map_err(|error| std::io::Error::other(format!("{error:?}")))?;

    record
        .validate_for_persistence()
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let restored = serde_json::from_slice::<TempMediaRecord>(&serde_json::to_vec(&record)?)?;
    assert_eq!(restored.delivery_route, record.delivery_route);
    assert_eq!(restored.authority, record.authority);
    Ok(())
}

#[test]
fn manual_recording_route_rejects_an_empty_asset_job_binding() {
    assert_eq!(
        TempMediaRecord::start_writing_with_route(
            "recording-1",
            OperationAuthority::Manual {
                profile_revision: "profile-7".to_owned(),
            },
            DeliveryRoute::LocalAsset {
                asset_job_id: String::new(),
            },
            1_000,
        ),
        Err(TempStoreError::InvalidTransition)
    );
}

#[test]
fn manual_recording_reader_cannot_be_projected_into_extension_reconnect()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let store =
        RecordingTransferStore::open(directory.path(), RecordingTransferStoreConfig::default())
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let media_path = store
        .start_recording_with_route_at(
            "recording-1",
            OperationAuthority::Manual {
                profile_revision: "profile-7".to_owned(),
            },
            DeliveryRoute::LocalAsset {
                asset_job_id: "asset-job-7".to_owned(),
            },
            1_000,
        )
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    std::fs::write(&media_path, b"\x00\x00\x00\x18ftypisommanual")?;
    store
        .finalize_manual_recording_at(
            "recording-1",
            RecordingMediaMetadata {
                recording_events: None,
                cursor_trajectory: None,
                filename: "recording-1.mp4".to_owned(),
                duration_ms: 500,
                width: 100,
                height: 100,
                fps: 30,
                open_editor: false,
            },
            2_000,
        )
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;

    let mut reader = store
        .manual_asset_reader("recording-1", "asset-job-7")
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    assert!(
        reader
            .read_next_plaintext(64)
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?
            .is_some()
    );
    assert_eq!(
        store
            .pending_recordings_for_reconnect_at(2_500)
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?
            .len(),
        0
    );
    Ok(())
}

#[test]
fn rejected_recordings_resume_only_after_explicit_bounded_desktop_recovery()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let store = RecordingTransferStore::open_with_default_config(directory.path())
        .map_err(|error| format!("{error:?}"))?;
    let mut saved = Vec::new();
    for index in 0..19 {
        let id = format!("recording-{index:02}");
        let route = if index == 18 {
            DeliveryRoute::LocalAsset {
                asset_job_id: "local-only".to_owned(),
            }
        } else {
            DeliveryRoute::DesktopDelivery {
                asset_job_id: "frozen-job".to_owned(),
                target_id: Some("b".repeat(64)),
                ask_after_capture: false,
            }
        };
        let path = store
            .start_recording_with_route_at(
                &id,
                OperationAuthority::Manual {
                    profile_revision: "frozen".to_owned(),
                },
                route,
                1_000,
            )
            .map_err(|error| format!("{error:?}"))?;
        std::fs::write(&path, b"retained MP4 source")?;
        store
            .finalize_manual_recording_at(
                &id,
                RecordingMediaMetadata {
                    recording_events: None,
                    cursor_trajectory: None,
                    filename: format!("{id}.mp4"),
                    duration_ms: 500,
                    width: 100,
                    height: 100,
                    fps: 30,
                    open_editor: false,
                },
                2_000,
            )
            .map_err(|error| format!("{error:?}"))?;
        store
            .recording_chunk_at(&id, 0, 2_500)
            .map_err(|error| format!("{error:?}"))?;
        store
            .reject_recording_at(&id, true, 3_000)
            .map_err(|error| format!("{error:?}"))?;
        let metadata = directory.path().join(format!("{id}.json"));
        let record: TempMediaRecord = serde_json::from_slice(&std::fs::read(&metadata)?)?;
        saved.push((metadata, path.clone(), record, std::fs::read(path)?));
    }
    assert_eq!(
        store
            .pending_recordings_for_reconnect_at(4_000)
            .map_err(|error| format!("{error:?}"))?,
        []
    );
    assert!(store.retry_rejected_desktop_at(2_999).is_err());
    store
        .retry_rejected_desktop_at(4_000)
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        store
            .pending_recordings_for_reconnect_at(4_000)
            .map_err(|error| format!("{error:?}"))?
            .len(),
        16
    );
    for (index, (metadata, path, mut expected, bytes)) in saved.into_iter().enumerate() {
        if index < 16 {
            expected.state = super::super::temp_media::TempMediaState::Finalized;
            expected.updated_at_ms = 4_000;
            expected.retain_rejected_until_ms = None;
        }
        let actual: TempMediaRecord = serde_json::from_slice(&std::fs::read(metadata)?)?;
        assert_eq!(actual, expected);
        assert_eq!(std::fs::read(path)?, bytes);
    }
    store
        .retry_rejected_desktop_at(5_000)
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        store
            .pending_recordings_for_reconnect_at(5_000)
            .map_err(|error| format!("{error:?}"))?
            .len(),
        18
    );
    Ok(())
}
