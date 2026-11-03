//! System-WebView lifecycle. Application state remains behind `DesktopBackend`.

mod commands;
mod windows;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tauri::Manager;

#[cfg(not(test))]
use super::model::DesktopBackend;

#[cfg(not(test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DesktopRuntimeError {
    #[cfg(sniptale_check_only)]
    BuildCheckOnly,
    Initialization,
    EventLoop,
}

#[derive(Debug, Default)]
struct RestoreWindows {
    capture: AtomicBool,
    settings: AtomicBool,
}

impl RestoreWindows {
    fn forget(&self, label: &str) {
        match label {
            "capture-panel" => self.capture.store(false, Ordering::Release),
            "application-settings" => self.settings.store(false, Ordering::Release),
            _ => {}
        }
    }
}

#[cfg(not(test))]
pub(crate) struct DesktopRuntime(tauri::App, Arc<RestoreWindows>);

#[cfg(not(test))]
impl DesktopRuntime {
    pub(crate) fn build(backend: Arc<dyn DesktopBackend>) -> Result<Self, DesktopRuntimeError> {
        let context = context()?;
        let restore = Arc::new(RestoreWindows::default());
        let window_restore = restore.clone();
        tauri::Builder::default()
            .manage(commands::Backend(backend))
            .on_window_event(move |window, event| {
                if matches!(event, tauri::WindowEvent::Destroyed)
                    || (window.label() == "capture-panel"
                        && matches!(event, tauri::WindowEvent::CloseRequested { .. }))
                {
                    window_restore.forget(window.label());
                }
            })
            .on_permission_request(|_, _| tauri::webview::PermissionResponse::Deny)
            .invoke_handler(tauri::generate_handler![
                commands::desktop_capture_snapshot,
                commands::desktop_capture_command,
                commands::desktop_preferences_snapshot,
                commands::desktop_preferences_save,
                commands::desktop_settings_command,
                commands::desktop_close_settings,
                commands::desktop_open_settings,
                commands::desktop_panel_frame,
                commands::desktop_close_panel,
                commands::desktop_drag_panel,
            ])
            .build(context)
            .map(|app| Self(app, restore))
            .map_err(|_| DesktopRuntimeError::Initialization)
    }

    pub(crate) fn handle(&self) -> DesktopHandle {
        DesktopHandle(self.0.handle().clone(), self.1.clone())
    }

    pub(crate) fn run(self) -> Result<(), DesktopRuntimeError> {
        let code = self.0.run_return(|_, event| {
            if let tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                // Closing the last WebView destroys presentation, not the resident agent.
                api.prevent_exit();
            }
        });
        if code == 0 {
            Ok(())
        } else {
            Err(DesktopRuntimeError::EventLoop)
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct DesktopHandle(tauri::AppHandle, Arc<RestoreWindows>);

impl DesktopHandle {
    #[cfg(not(test))]
    pub(crate) fn stop(&self) {
        self.0.exit(0);
    }
    pub(crate) fn hide_for_capture(&self) -> Result<(), super::model::DesktopError> {
        if let Some(window) = self.0.get_webview_window("application-settings") {
            let visible = window
                .is_visible()
                .map_err(|_| super::model::DesktopError::Unavailable)?;
            if visible {
                window
                    .hide()
                    .map_err(|_| super::model::DesktopError::Unavailable)?;
                self.1.settings.store(true, Ordering::Release);
            }
        }
        if let Some(window) = self.0.get_webview_window("capture-panel") {
            let visible = window
                .is_visible()
                .map_err(|_| super::model::DesktopError::Unavailable)?;
            if visible {
                window
                    .hide()
                    .map_err(|_| super::model::DesktopError::Unavailable)?;
                self.1.capture.store(true, Ordering::Release);
            }
        }
        Ok(())
    }
    pub(crate) fn restore_capture(&self) -> Result<(), super::model::DesktopError> {
        if self.1.capture.swap(false, Ordering::AcqRel)
            && let Some(window) = self.0.get_webview_window("capture-panel")
        {
            window
                .show()
                .map_err(|_| super::model::DesktopError::Unavailable)?;
        }
        Ok(())
    }
    pub(crate) fn restore_settings(&self) -> Result<(), super::model::DesktopError> {
        if self.1.settings.swap(false, Ordering::AcqRel)
            && let Some(window) = self.0.get_webview_window("application-settings")
        {
            window
                .show()
                .map_err(|_| super::model::DesktopError::Unavailable)?;
        }
        Ok(())
    }
    pub(crate) fn request_settings_close(&self) -> Result<(), super::model::DesktopError> {
        if let Some(window) = self.0.get_webview_window("application-settings") {
            window
                .close()
                .map_err(|_| super::model::DesktopError::Unavailable)?;
        }
        Ok(())
    }
    pub(crate) fn show_capture(&self) -> Result<(), super::model::DesktopError> {
        windows::show_capture(&self.0)
    }
    pub(crate) fn show_settings(&self) -> Result<(), super::model::DesktopError> {
        windows::show_settings(&self.0)
    }
    pub(crate) fn publish_capture(
        &self,
        snapshot: &super::model::CaptureSnapshot,
    ) -> Result<(), super::model::DesktopError> {
        use tauri::Emitter;
        if self.0.get_webview_window("capture-panel").is_some() {
            self.0
                .emit_to("capture-panel", "desktop-capture-state", snapshot)
                .map_err(|_| super::model::DesktopError::Unavailable)?;
        }
        Ok(())
    }
    pub(crate) fn publish_preferences(
        &self,
        preferences: super::model::UiPreferences,
    ) -> Result<(), super::model::DesktopError> {
        use tauri::Emitter;
        for label in ["capture-panel", "application-settings"] {
            if let Some(window) = self.0.get_webview_window(label) {
                windows::apply_preferences(&window, preferences)?;
                self.0
                    .emit_to(label, "desktop-preferences", preferences)
                    .map_err(|_| super::model::DesktopError::Unavailable)?;
            }
        }
        Ok(())
    }
}

#[cfg(all(not(test), not(sniptale_check_only)))]
fn context() -> Result<tauri::Context<tauri::Wry>, DesktopRuntimeError> {
    Ok(tauri::tauri_build_context!())
}

#[cfg(all(not(test), sniptale_check_only))]
fn context() -> Result<tauri::Context<tauri::Wry>, DesktopRuntimeError> {
    // Compile-only artifacts must never start a partially generated presentation runtime.
    Err(DesktopRuntimeError::BuildCheckOnly)
}

#[cfg(not(test))]
impl std::fmt::Debug for DesktopRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DesktopRuntime")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod restore_tests {
    use super::RestoreWindows;
    use std::sync::atomic::Ordering;

    #[test]
    fn capture_close_preserves_the_separate_settings_restore_intent() {
        let restore = RestoreWindows::default();
        restore.capture.store(true, Ordering::Release);
        restore.settings.store(true, Ordering::Release);
        restore.forget("capture-panel");
        assert!(!restore.capture.swap(false, Ordering::AcqRel));
        assert!(restore.settings.swap(false, Ordering::AcqRel));
        assert!(!restore.settings.swap(false, Ordering::AcqRel));
    }

    #[test]
    fn destroyed_settings_cannot_restore_or_clear_the_capture_intent() {
        let restore = RestoreWindows::default();
        restore.capture.store(true, Ordering::Release);
        restore.settings.store(true, Ordering::Release);
        restore.forget("application-settings");
        assert!(!restore.settings.swap(false, Ordering::AcqRel));
        assert!(restore.capture.swap(false, Ordering::AcqRel));
    }
}
