//! Popup-menu invocation for the hidden tray window.
#![allow(unsafe_code)]

use windows::Win32::{
    Foundation::{HWND, POINT},
    Graphics::Dwm::DwmFlush,
    UI::WindowsAndMessaging::{
        GetCursorPos, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu,
    },
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
    unsafe { GetCursorPos(&mut point) }.map_err(|_| TrayRuntimeError::PollFailed)?;
    // SAFETY: foreground activation is requested only for the adapter-owned hidden tray window.
    let _ = unsafe { SetForegroundWindow(owner) };
    // SAFETY: the menu and owner are adapter-owned; TrackPopupMenu retains neither handle.
    let command = unsafe {
        TrackPopupMenu(
            menu.handle(),
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            Some(0),
            owner,
            None,
        )
    };
    // SAFETY: TrackPopupMenu has returned and released its nested menu loop. Waiting for DWM to
    // consume the dismissal prevents the popup pixels from leaking into a capture/recording frame
    // dispatched by the selected command immediately afterward.
    unsafe { DwmFlush() }.map_err(|_| TrayRuntimeError::PollFailed)?;
    Ok((command.0 != 0).then_some(command.0 as u32))
}
