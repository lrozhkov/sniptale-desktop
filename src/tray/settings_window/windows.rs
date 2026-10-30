//! Resizable per-monitor-DPI Win32 settings-window adapter.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::{
    cell::RefCell,
    collections::BTreeMap,
    io,
    path::PathBuf,
    rc::{Rc, Weak},
};

use windows::{
    Win32::{
        Foundation::{
            ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
        },
        Graphics::Gdi::HBRUSH,
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, IDC_ARROW, IDCANCEL, IDOK,
            IsDialogMessageW, LoadCursorW, MSG, RegisterClassW, SW_HIDE, SW_SHOWNORMAL,
            SetForegroundWindow, SetWindowTextW, ShowWindow, WM_CHAR, WM_CLOSE, WM_COMMAND,
            WM_CTLCOLORBTN, WM_CTLCOLORSTATIC, WM_DPICHANGED, WM_DRAWITEM, WM_ERASEBKGND,
            WM_GETMINMAXINFO, WM_KEYDOWN, WM_KEYUP, WM_MOUSEWHEEL, WM_NCDESTROY, WM_PAINT,
            WM_SETTINGCHANGE, WM_SIZE, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_THEMECHANGED, WM_VSCROLL,
            WNDCLASSW, WS_EX_CONTROLPARENT, WS_OVERLAPPEDWINDOW, WS_VSCROLL,
        },
    },
    core::PCWSTR,
};

use super::model::{SettingsWindowEvent, SettingsWindowPresentation};

#[path = "windows/controls.rs"]
mod controls;
#[path = "windows/copy.rs"]
mod copy;
#[path = "windows/event_map.rs"]
mod event_map;
#[path = "windows/folder_picker.rs"]
pub(crate) mod folder_picker;
#[path = "windows/geometry.rs"]
mod geometry;
#[path = "windows/messages.rs"]
mod messages;
#[path = "windows/render.rs"]
mod render;
#[path = "windows/support.rs"]
mod support;
#[path = "windows/theme.rs"]
mod theme;

use controls::SettingsNativeWindow;
use support::{to_io_error, wide_null};

use super::dialog_commands::{APPLY_ID, BROWSE_DESTINATION_ID, CANCEL_ID};

// IsDialogMessageW synthesizes these commands for Enter and Escape respectively.
const _: () = assert!(APPLY_ID == IDOK.0 as usize && CANCEL_ID == IDCANCEL.0 as usize);
pub(super) const EXTENSION_MODE_ID: usize = 4;
pub(super) const SCREENSHOT_CURSOR_ID: usize = 5;
pub(super) const VIDEO_QUALITY_ID: usize = 6;
pub(super) const VIDEO_FPS_ID: usize = 7;
pub(super) const VIDEO_BITRATE_ID: usize = 8;
pub(super) const VIDEO_HARDWARE_ID: usize = 9;
pub(super) const VIDEO_CURSOR_ID: usize = 10;
pub(super) const VIDEO_DURATION_ID: usize = 11;
pub(super) const AUDIO_SOURCE_ID: usize = 12;
pub(super) const AUDIO_BITRATE_ID: usize = 13;
pub(super) const MICROPHONE_DEVICE_ID: usize = 14;
pub(super) const REFRESH_MICROPHONES_ID: usize = 15;
pub(super) const CONFIRM_DISCARD_ID: usize = 16;
pub(super) const KEEP_EDITING_ID: usize = 17;
pub(super) const THEME_ID: usize = 18;
pub(super) const NAV_FIRST_ID: usize = 30;
pub(super) const HOTKEY_FIRST_ID: usize = 100;

thread_local! {
    static WINDOW_EVENTS: RefCell<Vec<SettingsWindowEvent>> = const { RefCell::new(Vec::new()) };
    static WINDOW_STATES: RefCell<BTreeMap<usize, Weak<RefCell<SettingsNativeWindow>>>> = const { RefCell::new(BTreeMap::new()) };
}

#[derive(Debug, Default)]
pub(crate) struct WindowsSettingsWindow {
    window: Option<Rc<RefCell<SettingsNativeWindow>>>,
}

impl WindowsSettingsWindow {
    pub(crate) fn show(&mut self, presentation: SettingsWindowPresentation) -> io::Result<()> {
        let title = wide_null(copy::title(presentation.view()));
        if self.window.as_ref().is_none_or(|window| {
            window
                .try_borrow()
                .map_or(true, |window| window.handle.is_invalid())
        }) {
            self.window = Some(self.create_window(&title, presentation.clone())?);
        }
        let window = self
            .window
            .as_ref()
            .ok_or_else(|| io::Error::other("settings window creation failed"))?;
        let mut window = window
            .try_borrow_mut()
            .map_err(|_| io::Error::other("settings window is busy"))?;
        // SAFETY: adapter-owned HWND and NUL-terminated title are live for this call.
        unsafe { SetWindowTextW(window.handle, PCWSTR(title.as_ptr())) }.map_err(to_io_error)?;
        window.refresh(presentation)?;
        // SAFETY: showing and activating follows a user-initiated tray command.
        let _ = unsafe { ShowWindow(window.handle, SW_SHOWNORMAL) };
        let _ = unsafe { SetForegroundWindow(window.handle) };
        Ok(())
    }

    pub(crate) fn hide(&self) -> io::Result<()> {
        if let Some(handle) = self.handle() {
            if let Some(window) = self.window.as_ref()
                && let Ok(mut window) = window.try_borrow_mut()
            {
                window.cancel_hotkey_capture();
            }
            // SAFETY: hiding retains the modeless adapter-owned settings window.
            let _ = unsafe { ShowWindow(handle, SW_HIDE) };
        }
        Ok(())
    }

    pub(crate) fn drain_events(&mut self) -> Vec<SettingsWindowEvent> {
        WINDOW_EVENTS.with(|events| std::mem::take(&mut *events.borrow_mut()))
    }

    pub(crate) fn pick_destination(
        &self,
    ) -> Result<Option<PathBuf>, folder_picker::FolderPickerError> {
        folder_picker::pick_folder(self.handle())
    }

    pub(crate) fn dispatch_dialog_message(&self, message: &MSG) -> bool {
        let Some(handle) = self.handle() else {
            return false;
        };
        // SAFETY: the MSG comes from this UI thread and the modeless HWND is adapter-owned.
        unsafe { IsDialogMessageW(handle, message) }.as_bool()
    }

    fn create_window(
        &self,
        title: &[u16],
        presentation: SettingsWindowPresentation,
    ) -> io::Result<Rc<RefCell<SettingsNativeWindow>>> {
        let class_name = wide_null("SniptaleSettingsWindowV2");
        let module = current_module()?;
        register_class(module, &class_name)?;
        let bounds = geometry::initial_bounds()?;
        // SAFETY: class/title strings are NUL-terminated and no Rust pointer is retained.
        let handle = unsafe {
            CreateWindowExW(
                WS_EX_CONTROLPARENT,
                PCWSTR(class_name.as_ptr()),
                PCWSTR(title.as_ptr()),
                WS_OVERLAPPEDWINDOW | WS_VSCROLL,
                bounds.left,
                bounds.top,
                bounds.right - bounds.left,
                bounds.bottom - bounds.top,
                None,
                None,
                Some(module),
                None,
            )
        }
        .map_err(to_io_error)?;
        let state = match SettingsNativeWindow::create(handle, presentation) {
            Ok(state) => Rc::new(RefCell::new(state)),
            Err(error) => {
                // SAFETY: creation failed after this adapter created the HWND.
                let _ = unsafe { DestroyWindow(handle) };
                return Err(error);
            }
        };
        register_state(handle, &state);
        crate::tray::icon::windows::apply_main_icon_to_window(handle);
        Ok(state)
    }

    fn handle(&self) -> Option<HWND> {
        self.window
            .as_ref()
            .and_then(|window| window.try_borrow().ok().map(|window| window.handle))
            .filter(|handle| !handle.is_invalid())
    }
}

impl Drop for WindowsSettingsWindow {
    fn drop(&mut self) {
        if let Some(handle) = self.handle() {
            unregister_state(handle);
            // SAFETY: this adapter created the HWND and destroys it exactly once.
            let _ = unsafe { DestroyWindow(handle) };
        }
        self.window = None;
    }
}

fn register_state(handle: HWND, state: &Rc<RefCell<SettingsNativeWindow>>) {
    WINDOW_STATES.with(|states| {
        states
            .borrow_mut()
            .insert(handle.0 as usize, Rc::downgrade(state));
    });
}

fn unregister_state(handle: HWND) {
    WINDOW_STATES.with(|states| {
        states.borrow_mut().remove(&(handle.0 as usize));
    });
}

fn with_state<R>(handle: HWND, mutate: impl FnOnce(&mut SettingsNativeWindow) -> R) -> Option<R> {
    let state = WINDOW_STATES.with(|states| {
        states
            .borrow()
            .get(&(handle.0 as usize))
            .and_then(Weak::upgrade)
    })?;
    state
        .try_borrow_mut()
        .ok()
        .map(|mut state| mutate(&mut state))
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_CLOSE => {
            push_event(SettingsWindowEvent::RequestClose);
            LRESULT(0)
        }
        WM_COMMAND => messages::command(window, wparam, lparam),
        WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP => {
            messages::key(window, message, wparam, lparam)
        }
        WM_CHAR if messages::capture_active(window) => LRESULT(0),
        WM_PAINT => messages::paint(window),
        WM_SIZE => messages::resize(window, lparam),
        WM_DPICHANGED => geometry::dpi_changed(window, lparam),
        WM_MOUSEWHEEL => messages::mouse_wheel(window, wparam),
        WM_VSCROLL => messages::scroll(window, wparam),
        WM_CTLCOLORBTN | WM_CTLCOLORSTATIC => messages::static_color(window, wparam, lparam),
        WM_DRAWITEM => messages::draw_item(window, lparam),
        WM_SETTINGCHANGE | WM_THEMECHANGED => messages::refresh_theme(window),
        WM_GETMINMAXINFO => geometry::minmax(window, lparam),
        WM_ERASEBKGND => LRESULT(1),
        WM_NCDESTROY => {
            unregister_state(window);
            // SAFETY: default processing completes native non-client teardown.
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
        _ => {
            // SAFETY: unhandled messages follow the standard Win32 default procedure.
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
    }
}

fn current_module() -> io::Result<HINSTANCE> {
    // SAFETY: querying this process module handle retains no pointers.
    unsafe { GetModuleHandleW(None) }
        .map(|module| HINSTANCE(module.0))
        .map_err(to_io_error)
}

fn register_class(module: HINSTANCE, class_name: &[u16]) -> io::Result<()> {
    // SAFETY: predefined cursor is a shared process-independent resource.
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }.ok();
    let class = WNDCLASSW {
        hInstance: module,
        lpfnWndProc: Some(window_proc),
        lpszClassName: PCWSTR(class_name.as_ptr()),
        hbrBackground: HBRUSH::default(),
        hCursor: cursor.unwrap_or_default(),
        ..WNDCLASSW::default()
    };
    // SAFETY: RegisterClassW copies all class values synchronously.
    let atom = unsafe { RegisterClassW(&class) };
    if atom != 0 || unsafe { GetLastError() } == ERROR_CLASS_ALREADY_EXISTS {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

fn push_event(event: SettingsWindowEvent) {
    WINDOW_EVENTS.with(|events| events.borrow_mut().push(event));
}
