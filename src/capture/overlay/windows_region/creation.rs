//! Win32 class registration and overlay construction.
#![cfg(windows)]
#![allow(unsafe_code)]

use windows::{
    Win32::{
        Foundation::{COLORREF, ERROR_CLASS_ALREADY_EXISTS, GetLastError, HINSTANCE, HWND},
        Graphics::Gdi::{BLACK_BRUSH, GetStockObject, HBRUSH, UpdateWindow},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::KeyboardAndMouse::SetFocus,
            WindowsAndMessaging::{
                CreateWindowExW, IDC_CROSS, LWA_ALPHA, LoadCursorW, RegisterClassW, SW_SHOW,
                SetForegroundWindow, SetLayeredWindowAttributes, ShowWindow, WNDCLASSW,
                WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
            },
        },
    },
    core::PCWSTR,
};

use super::{
    OVERLAY_CLASS_NAME, WindowsRegionOverlay, state::OverlayWindowState, values::wide_null,
    window_proc,
};
use crate::capture::{coordinates::PhysicalRect, screenshot::ScreenshotCaptureError};

const OVERLAY_ALPHA: u8 = 72;

impl WindowsRegionOverlay {
    pub(super) fn create(bounds: PhysicalRect) -> Result<Self, ScreenshotCaptureError> {
        let class_name = wide_null(OVERLAY_CLASS_NAME);
        let instance = register_overlay_class(&class_name)?;
        let mut state = Box::new(OverlayWindowState::new(bounds));
        let window = create_overlay_window(bounds, instance, &class_name, state.as_mut())?;
        let overlay = Self {
            window,
            state: Some(state),
        };
        configure_overlay_window(window)?;
        Ok(overlay)
    }
}

fn register_overlay_class(class_name: &[u16]) -> Result<HINSTANCE, ScreenshotCaptureError> {
    // SAFETY: querying the current module handle does not retain pointers.
    let module = unsafe { GetModuleHandleW(None) }
        .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    let instance = HINSTANCE(module.0);
    let cursor = unsafe { LoadCursorW(None, IDC_CROSS) }.unwrap_or_default();
    let class = WNDCLASSW {
        hInstance: instance,
        hCursor: cursor,
        hbrBackground: HBRUSH(unsafe { GetStockObject(BLACK_BRUSH).0 }),
        lpfnWndProc: Some(window_proc),
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..WNDCLASSW::default()
    };
    // SAFETY: class metadata is process-local; duplicate registration is expected on later runs.
    if unsafe { RegisterClassW(&class) } == 0
        && unsafe { GetLastError() } != ERROR_CLASS_ALREADY_EXISTS
    {
        return Err(ScreenshotCaptureError::CaptureUnavailable);
    }
    Ok(instance)
}

fn create_overlay_window(
    bounds: PhysicalRect,
    instance: HINSTANCE,
    class_name: &[u16],
    state: &mut OverlayWindowState,
) -> Result<HWND, ScreenshotCaptureError> {
    let width =
        i32::try_from(bounds.width).map_err(|_| ScreenshotCaptureError::InvalidSelection)?;
    let height =
        i32::try_from(bounds.height).map_err(|_| ScreenshotCaptureError::InvalidSelection)?;
    // SAFETY: names remain live through the call. The boxed state address remains stable until
    // after the returned HWND is destroyed, including synchronous creation callbacks.
    let window = unsafe {
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(class_name.as_ptr()),
            WS_POPUP,
            bounds.x,
            bounds.y,
            width,
            height,
            None,
            None,
            Some(instance),
            Some(std::ptr::from_mut(state).cast()),
        )
    }
    .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    if window.is_invalid() {
        return Err(ScreenshotCaptureError::CaptureUnavailable);
    }
    Ok(window)
}

fn configure_overlay_window(window: HWND) -> Result<(), ScreenshotCaptureError> {
    // SAFETY: the caller has established RAII ownership before this first fallible call.
    unsafe { SetLayeredWindowAttributes(window, COLORREF(0), OVERLAY_ALPHA, LWA_ALPHA) }
        .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    // SAFETY: HWND is valid and belongs to this thread.
    unsafe {
        let _ = ShowWindow(window, SW_SHOW);
        if !UpdateWindow(window).as_bool() {
            return Err(ScreenshotCaptureError::CaptureUnavailable);
        }
        let _ = SetForegroundWindow(window);
        let _ = SetFocus(Some(window));
    }
    Ok(())
}
