//! Win32 tray command source adapter.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::{
    ffi::OsStr,
    io,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
};

use windows::{
    Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, RegisterWindowMessageW, TranslateMessage,
        WM_APP,
    },
    core::PCWSTR,
};

use super::source::{TrayCommandSource, TrayRuntimeError, TrayRuntimeEvent};
use crate::tray::{
    icon::{
        state::{TrayIconVariant, TrayNoticeKind},
        windows::{WindowsTrayIcon, WindowsTrayWindow, load_resource_icon},
    },
    menu::{render::TrayMenuRenderPlan, windows::WindowsTrayMenu},
    settings_window::model::SettingsWindowPresentation,
};

use super::activation::{TrayActivation, tray_activation};

const TRAY_CALLBACK_MESSAGE: u32 = WM_APP + 1;
const TRAY_ICON_ID: u32 = 1;
#[path = "windows/callbacks.rs"]
mod callbacks;
#[path = "windows/message_window.rs"]
mod message_window;
use message_window::WindowsMessageWindow;
#[path = "windows/foreground.rs"]
mod foreground;
#[path = "windows/open_folder.rs"]
mod open_folder;
#[path = "windows/popup.rs"]
mod popup;

#[derive(Debug)]
enum PendingWindow {
    Capture,
    Settings,
}

#[derive(Debug)]
pub(crate) struct WindowsTrayCommandSource {
    desktop: Option<crate::tray::desktop::DesktopHandle>,
    pending_window: Option<PendingWindow>,
    window: WindowsMessageWindow,
    icon: Option<WindowsTrayIcon>,
    menu: Option<WindowsTrayMenu>,
    taskbar_created_message: u32,
    tooltip: String,
    icon_variant: TrayIconVariant,
    foreground: foreground::ForegroundTarget,
}

impl WindowsTrayCommandSource {
    pub(crate) fn new() -> io::Result<Self> {
        let taskbar_created_message = register_taskbar_created_message()?;
        let window = WindowsMessageWindow::create()?;
        Ok(Self {
            desktop: None,
            pending_window: None,
            window,
            foreground: foreground::ForegroundTarget::default(),
            icon: None,
            menu: None,
            taskbar_created_message,
            tooltip: String::new(),
            icon_variant: TrayIconVariant::Normal,
        })
    }
}

impl TrayCommandSource for WindowsTrayCommandSource {
    fn attach_desktop(&mut self, desktop: crate::tray::desktop::DesktopHandle) {
        if let Some(pending) = self.pending_window.take() {
            let result = match pending {
                PendingWindow::Capture => desktop.show_capture(),
                PendingWindow::Settings => desktop.show_settings(),
            };
            if result.is_err() {
                let _ = self.show_notice(TrayNoticeKind::ActionRejected);
            }
        }
        self.desktop = Some(desktop);
    }
    fn capture_window_available(&self) -> bool {
        self.foreground.available()
    }

    fn finish_capture_dispatch(&mut self, accepted: bool) {
        if !accepted && let Some(desktop) = &self.desktop {
            let _ = desktop.restore_capture();
        }
    }

    fn prepare_capture(
        &mut self,
        action: crate::tray::ShellActionId,
    ) -> Result<(), TrayRuntimeError> {
        self.foreground.observe();
        if let Some(desktop) = &self.desktop {
            desktop
                .hide_for_capture()
                .map_err(|_| TrayRuntimeError::RenderFailed)?;
        }
        let restored = self.foreground.restore();
        if !restored
            && matches!(
                action,
                crate::tray::ShellActionId::ScreenshotActiveWindow
                    | crate::tray::ShellActionId::RecordingStartActiveWindow
            )
        {
            return Err(TrayRuntimeError::NoCaptureWindow);
        }
        foreground::settle().map_err(|_| TrayRuntimeError::RenderFailed)
    }

    fn choose_delivery_destination(
        &mut self,
        request: &crate::tray::delivery::DeliveryPromptRequest,
    ) -> crate::tray::delivery::DeliveryChoice {
        crate::tray::delivery::choose_delivery_destination(request)
    }
    fn apply_render_plan(
        &mut self,
        plan: &TrayMenuRenderPlan,
        tooltip: &str,
        icon_variant: TrayIconVariant,
    ) -> Result<(), TrayRuntimeError> {
        self.menu =
            Some(WindowsTrayMenu::from_plan(plan).map_err(|_| TrayRuntimeError::RenderFailed)?);
        if self.icon.is_none() {
            let icon =
                load_resource_icon(icon_variant).map_err(|_| TrayRuntimeError::RenderFailed)?;
            let tray_window = WindowsTrayWindow::new(self.window.handle())
                .map_err(|_| TrayRuntimeError::RenderFailed)?;
            let tray_icon = WindowsTrayIcon::add(
                tray_window,
                icon,
                TRAY_ICON_ID,
                TRAY_CALLBACK_MESSAGE,
                tooltip,
            )
            .map_err(|_| TrayRuntimeError::RenderFailed)?;
            self.icon = Some(tray_icon);
        } else if self.icon_variant != icon_variant {
            let icon =
                load_resource_icon(icon_variant).map_err(|_| TrayRuntimeError::RenderFailed)?;
            self.icon
                .as_mut()
                .ok_or(TrayRuntimeError::RenderFailed)?
                .update_icon(icon, tooltip)
                .map_err(|_| TrayRuntimeError::RenderFailed)?;
        } else if let Some(icon) = self.icon.as_ref() {
            icon.update_tooltip(tooltip)
                .map_err(|_| TrayRuntimeError::RenderFailed)?;
        }
        tooltip.clone_into(&mut self.tooltip);
        self.icon_variant = icon_variant;
        Ok(())
    }

    fn drain_events(&mut self) -> Result<Vec<TrayRuntimeEvent>, TrayRuntimeError> {
        self.foreground.observe();
        let mut events = Vec::new();
        self.drain_tray_callbacks(&mut events)?;
        loop {
            let mut message = MSG::default();
            // SAFETY: this drains messages for the hidden tray window owned by this adapter; MSG
            // storage lives for the duration of the call.
            let has_message =
                unsafe { PeekMessageW(&raw mut message, None, 0, 0, PM_REMOVE) }.as_bool();
            if !has_message {
                break;
            }
            if message.message == self.taskbar_created_message {
                if let Some(icon) = self.icon.as_mut() {
                    icon.restore_after_taskbar_restart(&self.tooltip)
                        .map_err(|_| TrayRuntimeError::PollFailed)?;
                }
            } else {
                // SAFETY: the message came from this adapter's thread queue. Translation and
                // dispatch are synchronous. Tauri owns its separate main-thread window loop.
                unsafe {
                    let _ = TranslateMessage(&raw const message);
                    DispatchMessageW(&raw const message);
                };
            }
            self.drain_tray_callbacks(&mut events)?;
        }
        Ok(events)
    }

    fn show_notice(&mut self, kind: TrayNoticeKind) -> Result<(), TrayRuntimeError> {
        let Some(icon) = self.icon.as_ref() else {
            return Err(TrayRuntimeError::RenderFailed);
        };
        icon.show_notice(kind)
            .map_err(|_| TrayRuntimeError::RenderFailed)
    }

    fn confirm_stop_recording_and_exit(&mut self) -> Result<bool, TrayRuntimeError> {
        Ok(crate::tray::desktop::show_prompt(
            crate::tray::desktop::prompts::PromptContent::QuitRecording,
            std::time::Duration::from_secs(30),
        ) == Some(1))
    }

    fn show_settings_window(
        &mut self,
        _presentation: SettingsWindowPresentation,
    ) -> Result<(), TrayRuntimeError> {
        if let Some(desktop) = &self.desktop {
            return desktop
                .show_settings()
                .map_err(|_| TrayRuntimeError::RenderFailed);
        }
        self.pending_window = Some(PendingWindow::Settings);
        Ok(())
    }

    fn hide_settings_window(&mut self) -> Result<(), TrayRuntimeError> {
        if let Some(desktop) = &self.desktop {
            desktop
                .request_settings_close()
                .map_err(|_| TrayRuntimeError::RenderFailed)?;
        }
        if matches!(self.pending_window, Some(PendingWindow::Settings)) {
            self.pending_window = None;
        }
        Ok(())
    }

    fn open_save_folder(&mut self, destination: &Path) -> Result<(), TrayRuntimeError> {
        open_folder::open(destination)
    }

    fn pick_settings_destination(&mut self) -> Result<Option<PathBuf>, TrayRuntimeError> {
        crate::tray::settings_window::windows::folder_picker::pick_folder(None)
            .map_err(|_| TrayRuntimeError::RenderFailed)
    }
}

impl WindowsTrayCommandSource {
    fn drain_tray_callbacks(
        &mut self,
        events: &mut Vec<TrayRuntimeEvent>,
    ) -> Result<(), TrayRuntimeError> {
        if let Some(activation) = callbacks::take().and_then(tray_activation) {
            match activation {
                TrayActivation::CapturePanel => {
                    if let Some(desktop) = &self.desktop {
                        desktop
                            .show_capture()
                            .map_err(|_| TrayRuntimeError::RenderFailed)?;
                    } else {
                        self.pending_window = Some(PendingWindow::Capture);
                    }
                }
                TrayActivation::ContextMenu => {
                    let command = popup::track_command(self.menu.as_ref(), self.window.handle());
                    callbacks::discard();
                    if let Some(command_id) = command? {
                        events.push(TrayRuntimeEvent::MenuCommand { command_id });
                    }
                }
            }
        }
        Ok(())
    }
}

fn register_taskbar_created_message() -> io::Result<u32> {
    let message = wide_null("TaskbarCreated");
    // SAFETY: the NUL-terminated message name remains valid for this synchronous registration.
    let registered = unsafe { RegisterWindowMessageW(PCWSTR(message.as_ptr())) };
    if registered == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(registered)
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain([0]).collect()
}
