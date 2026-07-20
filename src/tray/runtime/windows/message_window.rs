//! Hidden Win32 callback-window ownership for the tray adapter.
#![allow(unsafe_code)]

use std::{ffi::OsStr, io, os::windows::ffi::OsStrExt};

use windows::{
    Win32::{
        Foundation::{
            ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, WINDOW_EX_STYLE,
            WINDOW_STYLE, WM_CREATE, WM_DESTROY, WM_NCCREATE, WNDCLASSW,
        },
    },
    core::PCWSTR,
};

use super::{TRAY_CALLBACK_MESSAGE, callbacks};

#[derive(Debug)]
pub(super) struct WindowsMessageWindow {
    handle: HWND,
}

impl WindowsMessageWindow {
    pub(super) fn create() -> io::Result<Self> {
        let class_name = wide_null("SniptaleTrayCommandSource");
        let window_name = wide_null("Sniptale Tray");
        // SAFETY: querying this process module handle is synchronous and does not retain pointers.
        let module = unsafe { GetModuleHandleW(None) }.map_err(to_io_error)?;
        let instance = HINSTANCE(module.0);
        let class = WNDCLASSW {
            hInstance: instance,
            lpfnWndProc: Some(window_proc),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..WNDCLASSW::default()
        };
        // SAFETY: registration borrows the class for this call; duplicate registration is harmless.
        let atom = unsafe { RegisterClassW(&class) };
        if atom == 0 {
            // SAFETY: the error is read immediately after RegisterClassW reported failure.
            let error = unsafe { GetLastError() };
            if error != ERROR_CLASS_ALREADY_EXISTS {
                return Err(io::Error::from_raw_os_error(error.0 as i32));
            }
        }
        // SAFETY: names are NUL-terminated and valid for the synchronous create call.
        let handle = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                PCWSTR(class_name.as_ptr()),
                PCWSTR(window_name.as_ptr()),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                None,
                None,
                Some(instance),
                None,
            )
        }
        .map_err(to_io_error)?;
        if handle.is_invalid() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self { handle })
        }
    }

    pub(super) const fn handle(&self) -> HWND {
        self.handle
    }
}

impl Drop for WindowsMessageWindow {
    fn drop(&mut self) {
        if !self.handle.is_invalid() {
            // SAFETY: the adapter owns this hidden HWND and destroys it exactly once.
            let _ = unsafe { DestroyWindow(self.handle) };
        }
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == TRAY_CALLBACK_MESSAGE {
        callbacks::enqueue(lparam.0);
        return LRESULT(0);
    }
    match message {
        WM_NCCREATE => LRESULT(1),
        WM_CREATE | WM_DESTROY => LRESULT(0),
        _ => {
            // SAFETY: unhandled window messages must be passed to the system default procedure.
            unsafe { DefWindowProcW(window, message, wparam, lparam) }
        }
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain([0]).collect()
}

fn to_io_error(error: windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error(error.code().0)
}
