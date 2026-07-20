use crate::tray::TrayNoticeKind;
use crate::tray::runtime::browser::{
    BrowserLaunchMetadata, BrowserLaunchTarget, OpenSettingsFallbackPlan,
    plan_open_settings_fallback,
};

#[test]
fn open_settings_fallback_launches_exact_extension_settings_url() {
    let plan = plan_open_settings_fallback(Some(&BrowserLaunchMetadata {
        browser_available: true,
        settings_url: Some(
            "chrome-extension://abcdefghijklmnopqrstuvwxzyabcdef/settings/index.html#native-app"
                .to_owned(),
        ),
    }));

    assert_eq!(
        plan,
        OpenSettingsFallbackPlan {
            launch: Some(BrowserLaunchTarget::SettingsUrl(
                "chrome-extension://abcdefghijklmnopqrstuvwxzyabcdef/settings/index.html#native-app"
                    .to_owned()
            )),
            notice: None,
        }
    );
}

#[test]
fn open_settings_fallback_uses_browser_only_when_exact_url_is_unavailable() {
    for settings_url in [
        None,
        Some("https://example.invalid/private".to_owned()),
        Some("chrome-extension://bad/settings/index.html".to_owned()),
        Some("chrome-extension://abcdefghijklmnopqrstuvwxzyabcdef/settings with space".to_owned()),
    ] {
        let plan = plan_open_settings_fallback(Some(&BrowserLaunchMetadata {
            browser_available: true,
            settings_url,
        }));

        assert_eq!(
            plan,
            OpenSettingsFallbackPlan {
                launch: Some(BrowserLaunchTarget::BrowserOnly),
                notice: Some(TrayNoticeKind::ExtensionDisconnected),
            }
        );
    }
}

#[test]
fn open_settings_fallback_notifies_without_launch_when_browser_metadata_is_missing() {
    for metadata in [
        None,
        Some(BrowserLaunchMetadata {
            browser_available: false,
            settings_url: Some(
                "chrome-extension://abcdefghijklmnopqrstuvwxzyabcdef/settings/index.html"
                    .to_owned(),
            ),
        }),
    ] {
        let plan = plan_open_settings_fallback(metadata.as_ref());

        assert_eq!(
            plan,
            OpenSettingsFallbackPlan {
                launch: None,
                notice: Some(TrayNoticeKind::ExtensionDisconnected),
            }
        );
    }
}
