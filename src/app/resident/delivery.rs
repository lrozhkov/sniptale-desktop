//! Joins durable local publication with per-media browser authorization.

use crate::{
    protocol::DeliveryRoute,
    settings::ActiveSettings,
    transfer::browser_delivery::{BrowserDeliveryStore, BrowserMediaKind},
    tray::{
        TrayCommandSource, TrayNoticeKind,
        delivery::{DeliveryChoice, DeliveryPromptRequest},
    },
};

#[derive(Clone, Debug)]
pub(in crate::app) struct DesktopMediaDelivery {
    pub(super) media_id: String,
    pub(super) kind: BrowserMediaKind,
    pub(super) route: DeliveryRoute,
}

pub(super) fn finish_delivery(
    request: &DesktopMediaDelivery,
    local_saved: bool,
    settings: &ActiveSettings,
    source: &mut impl TrayCommandSource,
) {
    let DeliveryRoute::DesktopDelivery {
        target_id,
        ask_after_capture,
        ..
    } = &request.route
    else {
        return;
    };
    let Ok(registry) = settings.browser_targets() else {
        let _ = source.show_notice(TrayNoticeKind::ActionRejected);
        return;
    };
    if !local_saved {
        let _ = source.show_notice(TrayNoticeKind::LocalSaveFailed);
    }
    let choice = if *ask_after_capture {
        source.choose_delivery_destination(&DeliveryPromptRequest {
            targets: registry
                .targets()
                .iter()
                .map(|target| (target.id.clone(), target.label()))
                .collect(),
            local_saved,
        })
    } else {
        target_id.as_ref().map_or(DeliveryChoice::Local, |id| {
            DeliveryChoice::Browser(id.clone())
        })
    };
    match choice {
        DeliveryChoice::Browser(id) => {
            if !registry.targets().iter().any(|target| target.id == id) {
                let _ = source.show_notice(TrayNoticeKind::ActionRejected);
                return;
            }
            let selected = BrowserDeliveryStore::open_default().and_then(|store| {
                store.select(&request.media_id, request.kind, &request.route, &id)
            });
            if selected.is_err() || settings.activate_browser_delivery(&id).is_err() {
                let _ = source.show_notice(TrayNoticeKind::ActionRejected);
            } else {
                let _ = source.show_notice(TrayNoticeKind::WaitingForBrowser);
            }
        }
        DeliveryChoice::Local if local_saved => {
            if cleanup_published_source(request).is_err() {
                let _ = source.show_notice(TrayNoticeKind::ActionRejected);
            } else {
                let _ = source.show_notice(TrayNoticeKind::SavedLocally);
            }
        }
        DeliveryChoice::Local => {
            let _ = source.show_notice(TrayNoticeKind::ActionRejected);
        }
        DeliveryChoice::Later => {}
    }
}

fn cleanup_published_source(request: &DesktopMediaDelivery) -> Result<(), ()> {
    let job_id = request.route.local_asset_job_id().ok_or(())?;
    let result = match request.kind {
        BrowserMediaKind::Screenshot => {
            let store = crate::capture::ScreenshotTransferStore::open_default().map_err(|_| ())?;
            let _reader = store
                .manual_asset_reader(&request.media_id, job_id)
                .map_err(|_| ())?;
            store.discard_screenshot(&request.media_id).map_err(|_| ())
        }
        BrowserMediaKind::Recording => {
            let store = crate::recording::RecordingTransferStore::open_default().map_err(|_| ())?;
            let _reader = store
                .manual_asset_reader(&request.media_id, job_id)
                .map_err(|_| ())?;
            store.discard_recording(&request.media_id).map_err(|_| ())
        }
    };
    result?;
    cleanup_completed_browser_assignment(&request.media_id, request.kind);
    Ok(())
}

pub(in crate::app::resident) fn cleanup_completed_browser_assignment(
    media_id: &str,
    kind: BrowserMediaKind,
) {
    // The publication/ACK has already committed. Auxiliary cleanup failure cannot revoke it.
    let _ = BrowserDeliveryStore::open_default()
        .and_then(|store| store.remove_completed(media_id, kind));
}
