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
    assert!(
        store
            .pending_screenshots_for_reconnect_at(2_000)
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?
            .is_empty()
    );
    Ok(())
}
