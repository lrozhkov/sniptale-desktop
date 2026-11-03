//! Window creation admits local packaged documents only.

use super::super::model::DesktopError;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub(super) fn trusted_location(url: &tauri::Url) -> bool {
    let local = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
        || (url.scheme() == "http" && url.host_str() == Some("tauri.localhost"));
    local
        && url.port().is_none()
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(url.path(), "/" | "/index.html" | "/settings")
        && url.query().is_none()
}

pub(super) fn show_capture(app: &AppHandle) -> Result<(), DesktopError> {
    show(app, "capture-panel", "/", 760.0, 160.0, false)
}

pub(super) fn show_settings(app: &AppHandle) -> Result<(), DesktopError> {
    show(app, "application-settings", "/settings", 680.0, 600.0, true)
}

fn show(
    app: &AppHandle,
    label: &str,
    path: &str,
    width: f64,
    height: f64,
    decorated: bool,
) -> Result<(), DesktopError> {
    if let Some(window) = app.get_webview_window(label) {
        window
            .show()
            .and_then(|()| window.set_focus())
            .map_err(|_| DesktopError::Unavailable)?;
        return Ok(());
    }
    let preferences = app
        .state::<super::commands::Backend>()
        .0
        .preferences()
        .unwrap_or_default();
    WebviewWindowBuilder::new(app, label, WebviewUrl::App(path.into()))
        .title(window_title(preferences.locale, decorated)?)
        .theme(window_theme(preferences.theme))
        .inner_size(width, height)
        .transparent(!decorated)
        .shadow(decorated)
        .decorations(decorated)
        .resizable(decorated)
        .always_on_top(!decorated)
        .content_protected(!decorated)
        .skip_taskbar(!decorated)
        .center()
        .on_navigation(trusted_location)
        .build()
        .map(|_| ())
        .map_err(|_| DesktopError::Unavailable)
}

pub(super) fn apply_preferences(
    window: &tauri::WebviewWindow,
    preferences: super::super::model::UiPreferences,
) -> Result<(), DesktopError> {
    window
        .set_title(&window_title(
            preferences.locale,
            window.label() == "application-settings",
        )?)
        .and_then(|()| window.set_theme(window_theme(preferences.theme)))
        .map_err(|_| DesktopError::Unavailable)
}

fn window_title(locale: crate::settings::UiLocale, settings: bool) -> Result<String, DesktopError> {
    let tag = match locale {
        crate::settings::UiLocale::English => "en-US",
        crate::settings::UiLocale::Russian => "ru",
    };
    crate::tray::localization::Localizer::new(&[tag])
        .map(|copy| copy.window_title(settings))
        .map_err(|_| DesktopError::Unavailable)
}

fn window_theme(theme: crate::settings::AppearanceThemePreference) -> Option<tauri::Theme> {
    match theme {
        crate::settings::AppearanceThemePreference::System => None,
        crate::settings::AppearanceThemePreference::Light => Some(tauri::Theme::Light),
        crate::settings::AppearanceThemePreference::Dark => Some(tauri::Theme::Dark),
    }
}

#[cfg(test)]
mod tests {
    use super::trusted_location;
    #[test]
    fn reject_remote_documents_and_unrecognized_local_routes()
    -> Result<(), Box<dyn std::error::Error>> {
        for url in [
            "https://tauri.localhost/",
            "http://localhost/",
            "https://example.com/",
            "http://tauri.localhost:5173/",
            "tauri://localhost/sandbox",
            "tauri://localhost/settings?preview=1",
            "tauri://user@localhost/",
        ] {
            assert!(!trusted_location(&url.parse()?), "{url}");
        }
        for url in [
            "tauri://localhost/",
            "http://tauri.localhost/index.html",
            "http://tauri.localhost/settings",
        ] {
            assert!(trusted_location(&url.parse()?), "{url}");
        }
        Ok(())
    }
}

#[cfg(not(test))]
pub(super) fn apply_panel_frame(
    window: &tauri::WebviewWindow,
    request: super::super::panel_frame::PanelFrameRequest,
) -> Result<super::super::panel_frame::PanelInsets, DesktopError> {
    use super::super::panel_frame::FrameBounds;
    request.validate()?;
    let scale = request.pixel_ratio;
    let origin = window
        .outer_position()
        .map_err(|_| DesktopError::Unavailable)?;
    let size = window.inner_size().map_err(|_| DesktopError::Unavailable)?;
    let initial_origin = request.initial_origin(
        (f64::from(origin.x), f64::from(origin.y)),
        (f64::from(size.width), f64::from(size.height)),
    );
    let center_x = initial_origin.0 + (request.left + request.width / 2.0) * scale;
    let center_y = initial_origin.1 + (request.top + request.height / 2.0) * scale;
    let monitors = window
        .available_monitors()
        .map_err(|_| DesktopError::Unavailable)?;
    let monitor = monitors
        .iter()
        .find(|monitor| {
            let area = monitor.work_area();
            center_x >= f64::from(area.position.x)
                && center_x < f64::from(area.position.x) + f64::from(area.size.width)
                && center_y >= f64::from(area.position.y)
                && center_y < f64::from(area.position.y) + f64::from(area.size.height)
        })
        .cloned()
        .or(window
            .current_monitor()
            .map_err(|_| DesktopError::Unavailable)?)
        .ok_or(DesktopError::Unavailable)?;
    let area = monitor.work_area();
    let plan = request.place(
        initial_origin,
        FrameBounds {
            x: f64::from(area.position.x),
            y: f64::from(area.position.y),
            width: f64::from(area.size.width),
            height: f64::from(area.size.height),
        },
    )?;
    let desired_size = tauri::PhysicalSize::new(plan.width, plan.height).cast::<u32>();
    let desired_position = tauri::PhysicalPosition::new(plan.x, plan.y).cast::<i32>();
    if size != desired_size {
        window
            .set_size(desired_size)
            .map_err(|_| DesktopError::Unavailable)?;
    }
    if origin != desired_position {
        window
            .set_position(desired_position)
            .map_err(|_| DesktopError::Unavailable)?;
    }
    // Layout never shows a window: capture preparation owns temporary hiding and restoration.
    Ok(plan.insets)
}
