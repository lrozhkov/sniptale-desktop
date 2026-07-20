//! Win32 settings message handlers kept behind typed window-state methods.
#![allow(unsafe_code)]

use super::*;
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Gdi::{BeginPaint, EndPaint, HDC, PAINTSTRUCT},
    UI::{
        Controls::DRAWITEMSTRUCT,
        Input::KeyboardAndMouse::VIRTUAL_KEY,
        WindowsAndMessaging::{
            BN_CLICKED, CB_GETCURSEL, CBN_SELCHANGE, DefWindowProcW, SB_LINEDOWN, SB_LINEUP,
            SB_PAGEDOWN, SB_PAGEUP, SB_THUMBPOSITION, SB_THUMBTRACK, SendMessageW, WM_KEYUP,
            WM_SYSKEYUP,
        },
    },
};

pub(super) fn command(window: HWND, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let id = wparam.0 & 0xffff;
    let notification = (wparam.0 >> 16) as u32;
    if notification == BN_CLICKED && (HOTKEY_FIRST_ID..=HOTKEY_FIRST_ID + 7).contains(&id) {
        let _ = with_state(window, |state| {
            state.begin_hotkey_capture(id - HOTKEY_FIRST_ID)
        });
        return LRESULT(0);
    }
    let event = if notification == BN_CLICKED {
        event_map::button(id)
    } else if notification == CBN_SELCHANGE {
        let index = combo_index(HWND(lparam.0 as *mut _));
        with_state(window, |state| state.combo_event(id, index)).flatten()
    } else {
        None
    };
    if let Some(event) = event {
        push_event(event);
    }
    LRESULT(0)
}

pub(super) fn key(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if !capture_active(window) {
        // SAFETY: recorder did not consume the original message or its parameters.
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }
    let key = event_map::capture_key(VIRTUAL_KEY(wparam.0 as u16));
    let key_up = matches!(message, WM_KEYUP | WM_SYSKEYUP);
    if let Some(event) = with_state(window, |state| state.capture_key(key, key_up)).flatten() {
        push_event(event);
    }
    LRESULT(0)
}

pub(super) fn paint(window: HWND) -> LRESULT {
    let mut paint = PAINTSTRUCT::default();
    // SAFETY: BeginPaint/EndPaint are paired for this WM_PAINT dispatch.
    let _ = unsafe { BeginPaint(window, &mut paint) };
    let _ = with_state(window, |state| state.paint());
    let _ = unsafe { EndPaint(window, &paint) };
    LRESULT(0)
}

pub(super) fn resize(window: HWND, lparam: LPARAM) -> LRESULT {
    let width = (lparam.0 as u32) & 0xffff;
    let height = ((lparam.0 as u32) >> 16) & 0xffff;
    let _ = with_state(window, |state| state.resize(width, height));
    LRESULT(0)
}

pub(super) fn mouse_wheel(window: HWND, wparam: WPARAM) -> LRESULT {
    let delta = ((wparam.0 >> 16) as u16) as i16;
    let _ = with_state(window, |state| {
        state.scroll_by(-i32::from(delta.signum()) * 48)
    });
    LRESULT(0)
}

pub(super) fn scroll(window: HWND, wparam: WPARAM) -> LRESULT {
    let command = (wparam.0 & 0xffff) as i32;
    let position = ((wparam.0 >> 16) & 0xffff) as i32;
    let _ = with_state(window, |state| match command {
        value if value == SB_LINEUP.0 => state.scroll_by(-40),
        value if value == SB_LINEDOWN.0 => state.scroll_by(40),
        value if value == SB_PAGEUP.0 => state.scroll_by(-state.current_layout().viewport.height),
        value if value == SB_PAGEDOWN.0 => state.scroll_by(state.current_layout().viewport.height),
        value if value == SB_THUMBTRACK.0 || value == SB_THUMBPOSITION.0 => {
            state.scroll_to(position);
        }
        _ => {}
    });
    LRESULT(0)
}

pub(super) fn static_color(window: HWND, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    with_state(window, |state| {
        state.static_color(HWND(lparam.0 as *mut _), HDC(wparam.0 as *mut _))
    })
    .unwrap_or(LRESULT(0))
}

pub(super) fn draw_item(window: HWND, lparam: LPARAM) -> LRESULT {
    if lparam.0 == 0 {
        return LRESULT(0);
    }
    // SAFETY: WM_DRAWITEM provides a valid DRAWITEMSTRUCT for this synchronous dispatch.
    let item = unsafe { &*(lparam.0 as *const DRAWITEMSTRUCT) };
    let _ = with_state(window, |state| state.draw_item(item));
    LRESULT(1)
}

pub(super) fn refresh_theme(window: HWND) -> LRESULT {
    let _ = with_state(window, SettingsNativeWindow::refresh_system_theme);
    LRESULT(0)
}

pub(super) fn capture_active(window: HWND) -> bool {
    with_state(window, |state| state.capture_active()).unwrap_or(false)
}

fn combo_index(control: HWND) -> usize {
    // SAFETY: CB_GETCURSEL synchronously reads selection state from this child control.
    unsafe { SendMessageW(control, CB_GETCURSEL, None, None) }
        .0
        .max(0) as usize
}
