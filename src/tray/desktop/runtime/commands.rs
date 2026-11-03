//! Local-window authorization followed by typed application coordination.

use super::super::model::DesktopBackend;
#[cfg(not(test))]
use super::super::model::{
    CaptureRequest, CaptureSnapshot, DesktopError, PreferencesPatch, UiPreferences,
};
use std::sync::Arc;
#[cfg(not(test))]
use tauri::{Manager, State, WebviewWindow};

pub(super) struct Backend(pub(super) Arc<dyn DesktopBackend>);

#[cfg(not(test))]
fn authorize(window: &WebviewWindow, labels: &[&str]) -> Result<(), DesktopError> {
    let url = window.url().map_err(|_| DesktopError::UntrustedWindow)?;
    if !labels.contains(&window.label()) || !super::windows::trusted_location(&url) {
        return Err(DesktopError::UntrustedWindow);
    }
    Ok(())
}

#[cfg(not(test))]
#[tauri::command]
pub(super) async fn desktop_capture_snapshot(
    window: WebviewWindow,
    backend: State<'_, Backend>,
) -> Result<CaptureSnapshot, DesktopError> {
    authorize(&window, &["capture-panel"])?;
    drop(window);
    let backend = backend.0.clone();
    tauri::async_runtime::spawn_blocking(move || backend.snapshot())
        .await
        .map_err(|_| DesktopError::Unavailable)?
}

#[cfg(not(test))]
#[tauri::command]
pub(super) async fn desktop_capture_command(
    window: WebviewWindow,
    backend: State<'_, Backend>,
    request: CaptureRequest,
) -> Result<(), DesktopError> {
    authorize(&window, &["capture-panel"])?;
    drop(window);
    let backend = backend.0.clone();
    tauri::async_runtime::spawn_blocking(move || backend.command(request))
        .await
        .map_err(|_| DesktopError::Unavailable)?
}

#[cfg(not(test))]
#[tauri::command]
pub(super) async fn desktop_preferences_snapshot(
    window: WebviewWindow,
    backend: State<'_, Backend>,
) -> Result<UiPreferences, DesktopError> {
    authorize(&window, &["capture-panel", "application-settings"])?;
    drop(window);
    let backend = backend.0.clone();
    tauri::async_runtime::spawn_blocking(move || backend.preferences())
        .await
        .map_err(|_| DesktopError::Unavailable)?
}

#[cfg(not(test))]
#[tauri::command]
pub(super) async fn desktop_preferences_save(
    window: WebviewWindow,
    backend: State<'_, Backend>,
    patch: PreferencesPatch,
) -> Result<(), DesktopError> {
    authorize(&window, &["application-settings"])?;
    if patch.theme.is_none() && patch.locale.is_none() {
        return Err(DesktopError::InvalidPreferences);
    }
    drop(window);
    let backend = backend.0.clone();
    tauri::async_runtime::spawn_blocking(move || backend.save_preferences(patch))
        .await
        .map_err(|_| DesktopError::Unavailable)?
}

#[cfg(not(test))]
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command extraction supplies an owned WebviewWindow handle."
)]
pub(super) fn desktop_close_panel(window: WebviewWindow) -> Result<(), DesktopError> {
    authorize(&window, &["capture-panel"])?;
    window.destroy().map_err(|_| DesktopError::Unavailable)
}

#[cfg(not(test))]
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command extraction supplies an owned WebviewWindow handle."
)]
pub(super) fn desktop_drag_panel(window: WebviewWindow) -> Result<(), DesktopError> {
    authorize(&window, &["capture-panel"])?;
    window
        .start_dragging()
        .map_err(|_| DesktopError::Unavailable)
}

impl std::fmt::Debug for Backend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Backend").finish_non_exhaustive()
    }
}

#[cfg(not(test))]
#[tauri::command]
pub(super) async fn desktop_settings_command(
    window: WebviewWindow,
    backend: State<'_, Backend>,
    request: super::super::settings::SettingsRequest,
) -> Result<super::super::settings::SettingsSnapshot, DesktopError> {
    authorize(&window, &["application-settings"])?;
    drop(window);
    let backend = backend.0.clone();
    tauri::async_runtime::spawn_blocking(move || backend.settings(request))
        .await
        .map_err(|_| DesktopError::Unavailable)?
}

#[cfg(not(test))]
#[tauri::command]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri command extraction supplies an owned WebviewWindow handle."
)]
pub(super) fn desktop_close_settings(window: WebviewWindow) -> Result<(), DesktopError> {
    authorize(&window, &["application-settings"])?;
    window.destroy().map_err(|_| DesktopError::Unavailable)
}

// Window creation runs on the asynchronous command executor to avoid a WebView UI-thread wait.
#[cfg(not(test))]
#[tauri::command]
pub(super) async fn desktop_open_settings(window: WebviewWindow) -> Result<(), DesktopError> {
    authorize(&window, &["capture-panel"])?;
    let app = window.app_handle().clone();
    drop(window);
    tauri::async_runtime::spawn_blocking(move || super::windows::show_settings(&app))
        .await
        .map_err(|_| DesktopError::Unavailable)?
}

#[cfg(not(test))]
#[tauri::command]
pub(super) async fn desktop_panel_frame(
    window: WebviewWindow,
    request: super::super::panel_frame::PanelFrameRequest,
) -> Result<super::super::panel_frame::PanelInsets, DesktopError> {
    authorize(&window, &["capture-panel"])?;
    tauri::async_runtime::spawn_blocking(move || {
        super::windows::apply_panel_frame(&window, request)
    })
    .await
    .map_err(|_| DesktopError::Unavailable)?
}
