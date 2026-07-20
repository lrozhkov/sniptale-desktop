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
    Win32::{
        Foundation::LPARAM,
        UI::WindowsAndMessaging::{
            DispatchMessageW, IDYES, MB_ICONQUESTION, MB_SETFOREGROUND, MB_YESNO, MSG, MessageBoxW,
            PM_REMOVE, PeekMessageW, RegisterWindowMessageW, TranslateMessage, WM_APP,
        },
    },
    core::PCWSTR,
};

use super::source::{
    TrayCommandSource, TrayRuntimeError, TrayRuntimeEvent, is_tray_popup_callback,
};
use crate::tray::{
    icon::{
        state::{TrayIconVariant, TrayNoticeKind},
        windows::{WindowsTrayIcon, WindowsTrayWindow, load_resource_icon},
    },
    menu::{render::TrayMenuRenderPlan, windows::WindowsTrayMenu},
    settings_window::{model::SettingsWindowPresentation, windows::WindowsSettingsWindow},
};

const TRAY_CALLBACK_MESSAGE: u32 = WM_APP + 1;
const TRAY_ICON_ID: u32 = 1;
#[path = "windows/callbacks.rs"]
mod callbacks;
#[path = "windows/message_window.rs"]
mod message_window;
use message_window::WindowsMessageWindow;
#[path = "windows/open_folder.rs"]
mod open_folder;
#[path = "windows/popup.rs"]
mod popup;

#[derive(Debug)]
pub(crate) struct WindowsTrayCommandSource {
    window: WindowsMessageWindow,
    icon: Option<WindowsTrayIcon>,
    menu: Option<WindowsTrayMenu>,
    taskbar_created_message: u32,
    tooltip: String,
    icon_variant: TrayIconVariant,
    settings_window: WindowsSettingsWindow,
}

impl WindowsTrayCommandSource {
    pub(crate) fn new() -> io::Result<Self> {
        let taskbar_created_message = register_taskbar_created_message()?;
        Ok(Self {
            window: WindowsMessageWindow::create()?,
            icon: None,
            menu: None,
            taskbar_created_message,
            tooltip: String::new(),
            icon_variant: TrayIconVariant::Normal,
            settings_window: WindowsSettingsWindow::default(),
        })
    }
}

impl TrayCommandSource for WindowsTrayCommandSource {
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
        self.tooltip = tooltip.to_owned();
        self.icon_variant = icon_variant;
        Ok(())
    }

    fn drain_events(&mut self) -> Result<Vec<TrayRuntimeEvent>, TrayRuntimeError> {
        let mut events = Vec::new();
        events.extend(
            self.settings_window
                .drain_events()
                .into_iter()
                .map(|event| TrayRuntimeEvent::SettingsWindow { event }),
        );
        self.drain_tray_callbacks(&mut events)?;
        loop {
            let mut message = MSG::default();
            // SAFETY: this drains messages for the hidden tray window owned by this adapter; MSG
            // storage lives for the duration of the call.
            // SAFETY: both the hidden tray window and the modeless settings window are owned by
            // this thread. Draining the thread queue ensures their commands are dispatched.
            let has_message =
                unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool();
            if !has_message {
                break;
            }
            if message.message == self.taskbar_created_message {
                if let Some(icon) = self.icon.as_mut() {
                    icon.restore_after_taskbar_restart(&self.tooltip)
                        .map_err(|_| TrayRuntimeError::PollFailed)?;
                }
            } else {
                if !self.settings_window.dispatch_dialog_message(&message) {
                    // SAFETY: the message came from this adapter's thread queue. Translation and
                    // dispatch are synchronous and cover both owned top-level HWNDs.
                    unsafe {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    };
                }
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
        let title = wide_null("Sniptale");
        let content = wide_null("Stop and save the current recording, then exit Sniptale?");
        // SAFETY: the hidden tray HWND is live and both UTF-16 buffers remain valid for the
        // synchronous modal call. No pointer or raw handle crosses the adapter boundary.
        let result = unsafe {
            MessageBoxW(
                Some(self.window.handle()),
                PCWSTR(content.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_YESNO | MB_ICONQUESTION | MB_SETFOREGROUND,
            )
        };
        Ok(result == IDYES)
    }

    fn show_settings_window(
        &mut self,
        presentation: SettingsWindowPresentation,
    ) -> Result<(), TrayRuntimeError> {
        self.settings_window
            .show(presentation)
            .map_err(|_| TrayRuntimeError::RenderFailed)
    }

    fn hide_settings_window(&mut self) -> Result<(), TrayRuntimeError> {
        self.settings_window
            .hide()
            .map_err(|_| TrayRuntimeError::RenderFailed)
    }

    fn open_save_folder(&mut self, destination: &Path) -> Result<(), TrayRuntimeError> {
        open_folder::open(destination)
    }

    fn pick_settings_destination(&mut self) -> Result<Option<PathBuf>, TrayRuntimeError> {
        self.settings_window
            .pick_destination()
            .map_err(|_| TrayRuntimeError::RenderFailed)
    }
}

impl WindowsTrayCommandSource {
    fn drain_tray_callbacks(
        &self,
        events: &mut Vec<TrayRuntimeEvent>,
    ) -> Result<(), TrayRuntimeError> {
        if let Some(lparam) = callbacks::take()
            && is_popup_trigger(LPARAM(lparam))
        {
            let command = popup::track_command(self.menu.as_ref(), self.window.handle());
            // TrackPopupMenu runs a nested message loop. Explorer can emit the trailing mouse
            // callback from the same gesture while that loop is active; it must not reopen the
            // menu after its selected command has returned to the tray worker.
            callbacks::discard();
            if let Some(command_id) = command? {
                events.push(TrayRuntimeEvent::MenuCommand { command_id });
            }
        }
        Ok(())
    }
}

fn is_popup_trigger(lparam: LPARAM) -> bool {
    is_tray_popup_callback(lparam.0)
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
