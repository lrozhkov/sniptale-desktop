//! Popup-menu invocation for the hidden tray window.
#![allow(unsafe_code)]

use windows::Win32::{
    Foundation::{HWND, POINT},
    UI::WindowsAndMessaging::GetCursorPos,
};

use crate::tray::{menu::windows::WindowsTrayMenu, runtime::source::TrayRuntimeError};

pub(super) fn track_command(
    menu: Option<&WindowsTrayMenu>,
    owner: HWND,
) -> Result<Option<u32>, TrayRuntimeError> {
    let Some(menu) = menu else {
        return Ok(None);
    };
    let mut point = POINT::default();
    // SAFETY: stack point and adapter-owned HWND/HMENU are valid for this synchronous call.
    unsafe { GetCursorPos(&raw mut point) }.map_err(|_| TrayRuntimeError::PollFailed)?;
    menu.track_at(owner, point)
        .map_err(|_| TrayRuntimeError::PollFailed)
}
