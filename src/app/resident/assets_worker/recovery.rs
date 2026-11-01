//! Explicit recovery uses frozen destination jobs and retained protected sources.

use super::super::{WorkerCommand, delivery::DesktopMediaDelivery};
use crate::{
    assets::{AssetJobId, AssetPublishService},
    transfer::browser_delivery::BrowserMediaKind,
};
use std::sync::mpsc::SyncSender;

pub(super) fn recover(service: &AssetPublishService, tray: Option<&SyncSender<WorkerCommand>>) {
    if let Ok(store) = crate::capture::ScreenshotTransferStore::open_default()
        && store.retry_rejected_desktop().is_ok()
        && let Ok(pending) = store.pending_screenshots_for_reconnect()
    {
        for item in pending.into_iter().take(16) {
            let Some(route @ crate::protocol::DeliveryRoute::DesktopDelivery { .. }) =
                item.delivery_route
            else {
                continue;
            };
            let Some(bound_job) = route.local_asset_job_id() else {
                continue;
            };
            let (Ok(job_id), Ok(mut reader)) = (
                AssetJobId::parse(bound_job.to_owned()),
                store.manual_asset_reader(&item.capture_id, bound_job),
            ) else {
                continue;
            };
            let result = service.publish_finalized(&job_id, &mut reader);
            if let Some(sender) = tray {
                let _ = sender.try_send(WorkerCommand::ChooseMediaDestination {
                    request: Box::new(DesktopMediaDelivery {
                        media_id: item.capture_id,
                        kind: BrowserMediaKind::Screenshot,
                        route,
                    }),
                    local_saved: result.is_ok(),
                });
            }
        }
    }
    recover_recordings(service, tray);
}

fn recover_recordings(service: &AssetPublishService, tray: Option<&SyncSender<WorkerCommand>>) {
    if let Ok(store) = crate::recording::RecordingTransferStore::open_default()
        && store.retry_rejected_desktop().is_ok()
        && let Ok(pending) = store.pending_recordings_for_reconnect()
    {
        for item in pending.into_iter().take(16) {
            let Some(route @ crate::protocol::DeliveryRoute::DesktopDelivery { .. }) =
                item.delivery_route
            else {
                continue;
            };
            let Some(bound_job) = route.local_asset_job_id() else {
                continue;
            };
            let (Ok(job_id), Ok(mut reader)) = (
                AssetJobId::parse(bound_job.to_owned()),
                store.manual_asset_reader(&item.recording_id, bound_job),
            ) else {
                continue;
            };
            let result = service.publish_finalized(&job_id, &mut reader);
            if let Some(sender) = tray {
                let _ = sender.try_send(WorkerCommand::ChooseMediaDestination {
                    request: Box::new(DesktopMediaDelivery {
                        media_id: item.recording_id,
                        kind: BrowserMediaKind::Recording,
                        route,
                    }),
                    local_saved: result.is_ok(),
                });
            }
        }
    }
}
