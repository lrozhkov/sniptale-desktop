//! Win32 desktop source queries for WGC target resolution.
#![allow(unsafe_code)]

use std::ptr;

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, POINT, RECT},
        Graphics::{
            Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute},
            Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO},
        },
        UI::WindowsAndMessaging::{
            GetCursorPos, GetForegroundWindow, GetWindowRect, IsIconic, MONITORINFOF_PRIMARY,
        },
    },
    core::BOOL,
};

use super::super::super::super::{
    coordinates::{PhysicalPoint, PhysicalRect},
    screenshot::ScreenshotCaptureError,
    sources::{MonitorSnapshot, WindowCaptureState, WindowSnapshot},
};
use super::{NativeMonitor, NativeWindow};

pub(super) fn enumerate_monitors() -> Result<Vec<NativeMonitor>, ScreenshotCaptureError> {
    let mut monitors = Vec::new();
    // SAFETY: The callback receives the pointer provided in LPARAM for this synchronous call only.
    // The Vec outlives EnumDisplayMonitors, and the callback only pushes validated snapshots.
    let ok = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(enum_monitor_proc),
            LPARAM(ptr::addr_of_mut!(monitors).cast::<()>() as isize),
        )
    };
    if !ok.as_bool() || monitors.is_empty() {
        return Err(ScreenshotCaptureError::CaptureUnavailable);
    }
    Ok(monitors)
}

unsafe extern "system" fn enum_monitor_proc(
    monitor: HMONITOR,
    _dc: HDC,
    _rect: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: data is the Vec<NativeMonitor> pointer passed by enumerate_monitors for the duration
    // of EnumDisplayMonitors. Returning false stops enumeration on invalid monitor geometry.
    let monitors = unsafe { &mut *(data.0 as *mut Vec<NativeMonitor>) };
    match monitor_snapshot(monitor, monitors.len()) {
        Some(snapshot) => {
            monitors.push(NativeMonitor {
                handle: monitor,
                snapshot,
            });
            BOOL(1)
        }
        None => BOOL(0),
    }
}

fn monitor_snapshot(monitor: HMONITOR, index: usize) -> Option<MonitorSnapshot> {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..MONITORINFO::default()
    };
    // SAFETY: info points to a properly initialized MONITORINFO with cbSize set as required by
    // GetMonitorInfoW, and monitor is supplied by EnumDisplayMonitors.
    if !unsafe { GetMonitorInfoW(monitor, ptr::addr_of_mut!(info)) }.as_bool() {
        return None;
    }
    let bounds = rect_to_physical(info.rcMonitor)?;
    Some(MonitorSnapshot {
        id: format!("monitor-{index}"),
        bounds,
        is_primary: info.dwFlags & MONITORINFOF_PRIMARY != 0,
    })
}

pub(super) fn cursor_position() -> Option<PhysicalPoint> {
    let mut point = POINT::default();
    // SAFETY: point is a valid out-parameter for GetCursorPos.
    unsafe { GetCursorPos(ptr::addr_of_mut!(point)) }
        .ok()
        .map(|()| PhysicalPoint {
            x: point.x,
            y: point.y,
        })
}

pub(super) fn active_window() -> Option<NativeWindow> {
    // SAFETY: GetForegroundWindow has no preconditions and returns null when unavailable.
    let handle = unsafe { GetForegroundWindow() };
    if handle.is_invalid() {
        return None;
    }
    let state = window_capture_state(handle);
    let mut rect = RECT::default();
    // SAFETY: rect is a valid out-parameter and handle was returned by user32.
    unsafe { GetWindowRect(handle, ptr::addr_of_mut!(rect)) }.ok()?;
    Some(NativeWindow {
        handle,
        snapshot: WindowSnapshot {
            bounds: rect_to_physical(rect)?,
            capture_state: state,
        },
    })
}

fn window_capture_state(handle: HWND) -> WindowCaptureState {
    // SAFETY: handle was returned by user32; IsIconic is a read-only query.
    if unsafe { IsIconic(handle) }.as_bool() {
        return WindowCaptureState::Minimized;
    }
    let mut cloaked = 0_u32;
    // SAFETY: cloaked points to a u32 buffer of the exact size required by DWMWA_CLOAKED.
    if unsafe {
        DwmGetWindowAttribute(
            handle,
            DWMWA_CLOAKED,
            ptr::addr_of_mut!(cloaked).cast(),
            std::mem::size_of::<u32>() as u32,
        )
    }
    .is_ok()
        && cloaked != 0
    {
        return WindowCaptureState::Cloaked;
    }
    WindowCaptureState::Capturable
}

fn rect_to_physical(rect: RECT) -> Option<PhysicalRect> {
    let width = u32::try_from(rect.right.checked_sub(rect.left)?).ok()?;
    let height = u32::try_from(rect.bottom.checked_sub(rect.top)?).ok()?;
    PhysicalRect::new(rect.left, rect.top, width, height).ok()
}
