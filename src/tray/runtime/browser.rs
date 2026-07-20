//! Browser settings fallback launch planning.

use crate::tray::icon::state::TrayNoticeKind;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BrowserLaunchMetadata {
    pub(crate) browser_available: bool,
    pub(crate) settings_url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BrowserLaunchTarget {
    SettingsUrl(String),
    BrowserOnly,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OpenSettingsFallbackPlan {
    pub(crate) launch: Option<BrowserLaunchTarget>,
    pub(crate) notice: Option<TrayNoticeKind>,
}

#[must_use]
pub(crate) fn plan_open_settings_fallback(
    metadata: Option<&BrowserLaunchMetadata>,
) -> OpenSettingsFallbackPlan {
    let Some(metadata) = metadata else {
        return disconnected_without_launch();
    };
    if !metadata.browser_available {
        return disconnected_without_launch();
    }
    match metadata.settings_url.as_deref() {
        Some(url) if is_extension_settings_url(url) => OpenSettingsFallbackPlan {
            launch: Some(BrowserLaunchTarget::SettingsUrl(url.to_owned())),
            notice: None,
        },
        Some(_) | None => OpenSettingsFallbackPlan {
            launch: Some(BrowserLaunchTarget::BrowserOnly),
            notice: Some(TrayNoticeKind::ExtensionDisconnected),
        },
    }
}

fn disconnected_without_launch() -> OpenSettingsFallbackPlan {
    OpenSettingsFallbackPlan {
        launch: None,
        notice: Some(TrayNoticeKind::ExtensionDisconnected),
    }
}

fn is_extension_settings_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("chrome-extension://") else {
        return false;
    };
    let Some((extension_id, path)) = rest.split_once('/') else {
        return false;
    };
    extension_id.len() == 32
        && extension_id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() && byte.is_ascii_alphabetic())
        && !path.is_empty()
        && !url.bytes().any(|byte| byte.is_ascii_whitespace())
}
