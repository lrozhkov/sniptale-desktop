use super::super::transfer_record::ScreenshotTransferRecord;
use std::fs;

use tempfile::tempdir;

use super::super::{
    coordinates::PhysicalRect, transfer_record::FinalizedScreenshotTransfer,
    transfer_store::ScreenshotTransferStore,
};
use crate::protocol::{DeliveryRoute, OperationAuthority, ScreenshotMode};

#[test]
fn manual_screenshot_route_is_accepted_only_with_a_bound_local_asset_job()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let store = ScreenshotTransferStore::open_with_default_config(directory.path())
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let media_path = store
        .screenshot_media_path("capture-1")
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    fs::write(&media_path, b"\x89PNG\r\n\x1a\npixels")?;
    let (total_bytes, sha256, media_protection) = store
        .protect_screenshot_media("capture-1")
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let transfer = FinalizedScreenshotTransfer {
        capture_id: "capture-1".to_owned(),
        mode: ScreenshotMode::Screen,
        filename: "capture-1.png".to_owned(),
        bounds: PhysicalRect {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        },
        authority: OperationAuthority::Manual {
            profile_revision: "profile-4".to_owned(),
        },
        delivery_route: DeliveryRoute::LocalAsset {
            asset_job_id: "asset-job-4".to_owned(),
        },
        total_bytes,
        sha256,
        media_protection: Some(media_protection),
        open_editor: false,
    };

    store
        .write_finalized_capture_at(transfer, 1_000)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let mut reader = store
        .manual_asset_reader("capture-1", "asset-job-4")
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    assert!(
        reader
            .read_next_plaintext(64)
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?
            .is_some()
    );
    assert_eq!(
        reader
            .read_next_plaintext(64)
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?,
        None
    );
    assert_eq!(
        store
            .pending_screenshots_for_reconnect_at(2_000)
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?
            .len(),
        0
    );
    Ok(())
}

#[test]
fn explicit_desktop_recovery_resumes_at_most_sixteen_rejections_without_rebinding()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let store = ScreenshotTransferStore::open_with_default_config(directory.path())
        .map_err(|error| format!("{error:?}"))?;
    let route = DeliveryRoute::DesktopDelivery {
        asset_job_id: "asset-job-frozen".to_owned(),
        target_id: Some("a".repeat(64)),
        ask_after_capture: false,
    };
    let saved = (0..20)
        .map(|index| prepare_rejected_screenshot(&store, directory.path(), index, &route))
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(
        store
            .pending_screenshots_for_reconnect_at(4_000)
            .map_err(|error| format!("{error:?}"))?,
        []
    );
    assert!(store.retry_rejected_desktop_at(2_999).is_err());
    store
        .retry_rejected_desktop_at(4_000)
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        store
            .pending_screenshots_for_reconnect_at(4_000)
            .map_err(|error| format!("{error:?}"))?
            .len(),
        16
    );
    for (index, (metadata, path, mut expected, bytes)) in saved.into_iter().enumerate() {
        if index < 16 {
            expected.state = super::super::transfer_record::ScreenshotTransferState::Finalized;
            expected.updated_at_ms = 4_000;
            expected.retain_rejected_until_ms = None;
        }
        let actual: ScreenshotTransferRecord = serde_json::from_slice(&fs::read(metadata)?)?;
        assert_eq!(actual, expected);
        assert_eq!(fs::read(path)?, bytes);
    }
    store
        .retry_rejected_desktop_at(5_000)
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        store
            .pending_screenshots_for_reconnect_at(5_000)
            .map_err(|error| format!("{error:?}"))?
            .len(),
        18
    );
    Ok(())
}

type RetainedScreenshot = (
    std::path::PathBuf,
    std::path::PathBuf,
    ScreenshotTransferRecord,
    Vec<u8>,
);

fn prepare_rejected_screenshot(
    store: &ScreenshotTransferStore,
    directory: &std::path::Path,
    index: usize,
    route: &DeliveryRoute,
) -> Result<RetainedScreenshot, Box<dyn std::error::Error>> {
    let id = format!("capture-{index:02}");
    let path = store
        .screenshot_media_path(&id)
        .map_err(|error| format!("{error:?}"))?;
    fs::write(&path, b"retained PNG source")?;
    let (total_bytes, sha256, protection) = store
        .protect_screenshot_media(&id)
        .map_err(|error| format!("{error:?}"))?;
    let (authority, delivery_route) = match index {
        18 => (
            OperationAuthority::Manual {
                profile_revision: "frozen".to_owned(),
            },
            DeliveryRoute::LocalAsset {
                asset_job_id: "local-only".to_owned(),
            },
        ),
        19 => (
            OperationAuthority::ExtensionManaged {
                settings_revision: "legacy".to_owned(),
            },
            DeliveryRoute::ExtensionTransfer,
        ),
        _ => (
            OperationAuthority::Manual {
                profile_revision: "frozen".to_owned(),
            },
            route.clone(),
        ),
    };
    let transfer = FinalizedScreenshotTransfer {
        capture_id: id.clone(),
        mode: ScreenshotMode::Screen,
        filename: format!("{id}.png"),
        bounds: PhysicalRect {
            x: -100,
            y: 0,
            width: 100,
            height: 100,
        },
        authority,
        delivery_route,
        total_bytes,
        sha256,
        media_protection: Some(protection),
        open_editor: false,
    };
    store
        .write_finalized_capture_at(transfer, 1_000)
        .map_err(|error| format!("{error:?}"))?;
    store
        .screenshot_chunk_at(&id, 0, 2_000)
        .map_err(|error| format!("{error:?}"))?;
    store
        .reject_screenshot_at(&id, false, 3_000)
        .map_err(|error| format!("{error:?}"))?;
    let metadata = directory.join(format!("{id}.json"));
    let record: ScreenshotTransferRecord = serde_json::from_slice(&fs::read(&metadata)?)?;
    let bytes = fs::read(&path)?;
    Ok((metadata, path, record, bytes))
}
