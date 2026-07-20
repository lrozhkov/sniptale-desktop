//! DPI-aware top-level window bounds and suggested-rectangle application.
#![allow(unsafe_code)]

use std::io;

use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT},
    UI::{
        HiDpi::{AdjustWindowRectExForDpi, GetDpiForSystem, GetDpiForWindow},
        WindowsAndMessaging::{
            GetClientRect, MINMAXINFO, SPI_GETWORKAREA, SWP_NOACTIVATE, SWP_NOZORDER,
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SetWindowPos, SystemParametersInfoW,
            WS_EX_CONTROLPARENT, WS_OVERLAPPEDWINDOW, WS_VSCROLL,
        },
    },
};

use crate::tray::settings_window::layout::{MIN_HEIGHT, MIN_WIDTH};

pub(super) fn initial_bounds() -> io::Result<RECT> {
    // SAFETY: system DPI query has no retained state.
    let dpi = unsafe { GetDpiForSystem() }.max(96);
    let style = WS_OVERLAPPEDWINDOW | WS_VSCROLL;
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 960 * dpi as i32 / 96,
        bottom: 720 * dpi as i32 / 96,
    };
    // SAFETY: rect is initialized and adjusted synchronously for this style/DPI.
    unsafe { AdjustWindowRectExForDpi(&mut rect, style, false, WS_EX_CONTROLPARENT, dpi) }
        .map_err(super::support::to_io_error)?;
    let area = work_area();
    let width = (rect.right - rect.left).min((area.right - area.left) * 9 / 10);
    let height = (rect.bottom - rect.top).min((area.bottom - area.top) * 9 / 10);
    let left = area.left + (area.right - area.left - width) / 2;
    let top = area.top + (area.bottom - area.top - height) / 2;
    Ok(RECT {
        left,
        top,
        right: left + width,
        bottom: top + height,
    })
}

pub(super) fn dpi_changed(window: HWND, lparam: LPARAM) -> LRESULT {
    // SAFETY: WM_DPICHANGED supplies a valid suggested RECT pointer for this dispatch.
    let suggested = unsafe { &*(lparam.0 as *const RECT) };
    // SAFETY: applying suggested bounds keeps this adapter-owned window on the new monitor.
    let _ = unsafe {
        SetWindowPos(
            window,
            None,
            suggested.left,
            suggested.top,
            suggested.right - suggested.left,
            suggested.bottom - suggested.top,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
    };
    let mut client = RECT::default();
    // SAFETY: GetClientRect initializes one RECT for this live HWND.
    if unsafe { GetClientRect(window, &mut client) }.is_ok() {
        let _ = super::with_state(window, |state| {
            state.resize(client.right as u32, client.bottom as u32)
        });
    }
    LRESULT(0)
}

pub(super) fn minmax(window: HWND, lparam: LPARAM) -> LRESULT {
    if lparam.0 == 0 {
        return LRESULT(0);
    }
    // SAFETY: WM_GETMINMAXINFO supplies writable MINMAXINFO for this dispatch.
    let info = unsafe { &mut *(lparam.0 as *mut MINMAXINFO) };
    // SAFETY: querying DPI retains no pointers.
    let dpi = unsafe { GetDpiForWindow(window) }.max(96);
    let area = work_area();
    let max_width = ((area.right - area.left) * 9 / 10).max(480);
    let max_height = ((area.bottom - area.top) * 9 / 10).max(360);
    info.ptMinTrackSize.x = (MIN_WIDTH * dpi as i32 / 96).min(max_width);
    info.ptMinTrackSize.y = (MIN_HEIGHT * dpi as i32 / 96).min(max_height);
    LRESULT(0)
}

fn work_area() -> RECT {
    let mut area = RECT {
        left: 0,
        top: 0,
        right: 1280,
        bottom: 720,
    };
    // SAFETY: SPI_GETWORKAREA writes one RECT and retains no caller memory.
    let _ = unsafe {
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some((&raw mut area).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    area
}
