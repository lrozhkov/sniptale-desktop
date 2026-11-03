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
            let selected = BrowserDeliveryStore::open_default()
                .and_then(|store| select_browser_delivery(request, &id, settings, &store));
            if selected.is_err() {
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

fn select_browser_delivery(
    request: &DesktopMediaDelivery,
    target_id: &str,
    settings: &ActiveSettings,
    store: &BrowserDeliveryStore,
) -> Result<(), crate::transfer::browser_delivery::BrowserDeliveryError> {
    store.select(&request.media_id, request.kind, &request.route, target_id)?;
    // A prompt is an explicit Desktop choice. Replaying an automatic frozen route
    // must not preempt another browser's current transfer during a recovery batch.
    if matches!(
        request.route,
        DeliveryRoute::DesktopDelivery {
            ask_after_capture: true,
            ..
        }
    ) {
        settings
            .activate_browser_delivery(target_id)
            .map_err(|_| crate::transfer::browser_delivery::BrowserDeliveryError::Io)?;
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{BrowserFamily, NativeControllerIdentity};

    #[test]
    fn automatic_recovery_keeps_selection_and_explicit_prompt_can_change_it()
    -> Result<(), Box<dyn std::error::Error>> {
        for kind in [BrowserMediaKind::Screenshot, BrowserMediaKind::Recording] {
            let root = tempfile::tempdir()?;
            let registry = root.path().join("targets.json");
            let settings = ActiveSettings::extension_managed_default()
                .with_browser_target_store(registry.clone())
                .map_err(|_| "registry")?;
            let register = |family, profile: &str| {
                settings.register_browser_target(&NativeControllerIdentity {
                    extension_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
                    browser_family: family,
                    profile_key: profile.to_owned(),
                    connection_id: profile.to_owned(),
                    document_id: None,
                })
            };
            let chrome = register(BrowserFamily::Chrome, "chrome").map_err(|_| "chrome")?;
            let edge = register(BrowserFamily::Edge, "edge").map_err(|_| "edge")?;
            settings
                .select_browser_target(Some(&chrome.id), false)
                .map_err(|_| "select")?;
            let store = BrowserDeliveryStore::open(root.path().join("assignments"));
            for explicit in [false, true] {
                let request = DesktopMediaDelivery {
                    media_id: if explicit { "explicit" } else { "automatic" }.to_owned(),
                    kind,
                    route: DeliveryRoute::DesktopDelivery {
                        target_id: Some(edge.id.clone()),
                        ask_after_capture: explicit,
                        asset_job_id: "retained-job".to_owned(),
                    },
                };
                select_browser_delivery(&request, &edge.id, &settings, &store)
                    .map_err(|_| "delivery")?;
                assert_eq!(
                    store
                        .target(&request.media_id, kind)
                        .map_err(|_| "assignment")?,
                    Some(edge.id.clone())
                );
                let persisted = crate::settings::BrowserTargetRegistry::load(registry.clone())
                    .map_err(|_| "reload")?;
                assert_eq!(persisted.default_target_id(), Some(chrome.id.as_str()));
                assert_eq!(
                    persisted.selected_target_id(),
                    Some(if explicit {
                        edge.id.as_str()
                    } else {
                        chrome.id.as_str()
                    })
                );
            }
        }
        Ok(())
    }
}
