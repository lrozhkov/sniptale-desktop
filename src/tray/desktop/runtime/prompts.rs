//! Tauri prompt transport. Callers wait outside the `WebView` event loop.

#[cfg(not(test))]
use super::super::{
    model::DesktopError,
    prompts::{PromptReply, PromptView},
};
use super::{
    super::prompts::{PromptContent, PromptState},
    DesktopHandle,
};
use std::{
    sync::{Condvar, Mutex, OnceLock},
    time::{Duration, Instant},
};
use tauri::Manager;
#[cfg(not(test))]
use tauri::WebviewWindow;

type RegisteredDesktop = (Mutex<Option<DesktopHandle>>, Condvar);
static DESKTOP: OnceLock<RegisteredDesktop> = OnceLock::new();

#[cfg(not(test))]
pub(super) fn register(handle: DesktopHandle) {
    let (slot, ready) = DESKTOP.get_or_init(|| (Mutex::new(None), Condvar::new()));
    if let Ok(mut desktop) = slot.lock() {
        *desktop = Some(handle);
        ready.notify_all();
    }
}

pub(crate) fn show(content: PromptContent, timeout: Duration) -> Option<u8> {
    let started = Instant::now();
    let (slot, ready) = DESKTOP.get_or_init(|| (Mutex::new(None), Condvar::new()));
    let (desktop, _) = ready
        .wait_timeout_while(slot.lock().ok()?, timeout, |desktop| desktop.is_none())
        .ok()?;
    let handle = desktop.clone()?;
    drop(desktop);
    handle.prompt(content, timeout.checked_sub(started.elapsed())?)
}

impl DesktopHandle {
    pub(crate) fn prompt(&self, content: PromptContent, timeout: Duration) -> Option<u8> {
        let state = self.0.state::<PromptState>();
        let _serial = state.serial.try_lock().ok()?;
        let receiver = state.begin(content, timeout)?;
        let opened = super::windows::show(
            &self.0,
            "application-dialog",
            "/dialog",
            460.0,
            480.0,
            false,
        );
        let result = if opened.is_ok() {
            receiver.recv_timeout(timeout).ok()
        } else {
            None
        };
        if let Some(window) = self.0.get_webview_window("application-dialog") {
            if window.destroy().is_ok() {
                // destroy enqueues work on the UI loop. Keep the prompt reserved until
                // Destroyed arrives, so an old window cannot dismiss a new request.
                let _ = state.wait_closed(Duration::from_secs(2));
            } else {
                state.dismiss();
            }
        } else {
            state.end();
        }
        result
    }
}

#[cfg(not(test))]
#[tauri::command]
pub(super) fn desktop_prompt_snapshot(window: WebviewWindow) -> Result<PromptView, DesktopError> {
    super::commands::authorize(&window, &["application-dialog"])?;
    let app = window.app_handle().clone();
    drop(window);
    app.state::<PromptState>()
        .snapshot()
        .ok_or(DesktopError::Unavailable)
}

#[cfg(not(test))]
#[tauri::command]
pub(super) fn desktop_prompt_reply(
    window: WebviewWindow,
    reply: PromptReply,
) -> Result<(), DesktopError> {
    super::commands::authorize(&window, &["application-dialog"])?;
    let app = window.app_handle().clone();
    drop(window);
    if app.state::<PromptState>().reply(reply) {
        Ok(())
    } else {
        Err(DesktopError::Unavailable)
    }
}

#[cfg(not(test))]
#[tauri::command]
pub(super) fn desktop_prompt_dismiss(window: WebviewWindow) -> Result<(), DesktopError> {
    super::commands::authorize(&window, &["application-dialog"])?;
    let app = window.app_handle().clone();
    drop(window);
    app.state::<PromptState>().dismiss();
    Ok(())
}
