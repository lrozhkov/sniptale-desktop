//! Win32 region selection overlay adapter.
#![cfg(windows)]
#![allow(unsafe_code)]

mod creation;
mod lifetime;
mod service;
mod state;
mod values;

use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Dwm::DwmFlush,
    Graphics::Gdi::{
        BeginPaint, EndPaint, FrameRect, GetStockObject, HBRUSH, InvalidateRect, PAINTSTRUCT,
        WHITE_BRUSH,
    },
    UI::{
        Input::KeyboardAndMouse::{ReleaseCapture, SetCapture, VK_ESCAPE},
        WindowsAndMessaging::{
            CREATESTRUCTW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetMessageW,
            GetSystemMetrics, GetWindowLongPtrW, MSG, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
            SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SW_HIDE, SetWindowLongPtrW, ShowWindow,
            TranslateMessage, WM_CLOSE, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
            WM_NCCREATE, WM_NCDESTROY, WM_PAINT,
        },
    },
};

use super::{
    super::{
        ScreenshotCaptureRequest,
        coordinates::{PhysicalPoint, PhysicalRect},
        screenshot::{ScreenshotCaptureError, region::ScreenshotRegionSelector},
    },
    OverlayEvent,
};
use state::OverlayWindowState;
use values::mouse_point;

const OVERLAY_CLASS_NAME: &str = "SniptaleRegionOverlay";

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct WindowsRegionSelector;

impl ScreenshotRegionSelector for WindowsRegionSelector {
    fn select_region(
        &mut self,
        _request: &ScreenshotCaptureRequest,
    ) -> Result<PhysicalRect, ScreenshotCaptureError> {
        service::select_region()
    }
}

#[derive(Debug)]
pub(super) struct WindowsRegionOverlay {
    window: HWND,
    state: Option<Box<OverlayWindowState>>,
}

impl WindowsRegionOverlay {
    fn run_message_loop(&mut self) -> Result<(), ScreenshotCaptureError> {
        while !self
            .state
            .as_deref()
            .ok_or(ScreenshotCaptureError::CaptureUnavailable)?
            .has_outcome()
        {
            let mut message = MSG::default();
            // SAFETY: this modal loop runs on the current thread and dispatches messages for the
            // overlay window owned by this adapter; MSG storage lives for each call.
            let result = unsafe { GetMessageW(&mut message, Some(self.window), 0, 0) };
            if result.0 == -1 {
                return Err(ScreenshotCaptureError::CaptureUnavailable);
            }
            if result.0 == 0 {
                break;
            }
            // SAFETY: message was produced by GetMessageW for this thread and is dispatched
            // synchronously to the overlay window procedure.
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        Ok(())
    }

    pub(super) fn dismiss_and_flush(&self) -> Result<(), ScreenshotCaptureError> {
        // SAFETY: hiding the adapter-owned HWND is synchronous. DwmFlush then waits until the
        // compositor has observed the dismissal so neither the overlay nor its selection frame can
        // leak into the WGC frame captured immediately after this method returns.
        unsafe {
            let _ = ShowWindow(self.window, SW_HIDE);
            DwmFlush()
        }
        .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)
    }
}

pub(super) fn virtual_desktop_bounds() -> Result<PhysicalRect, ScreenshotCaptureError> {
    // SAFETY: GetSystemMetrics reads process-independent desktop metrics and does not retain state.
    let x = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let y = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    let width = u32::try_from(width).map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    let height = u32::try_from(height).map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    PhysicalRect::new(x, y, width, height).map_err(|_| ScreenshotCaptureError::CaptureUnavailable)
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        // SAFETY: lparam for WM_NCCREATE points to CREATESTRUCTW supplied by CreateWindowExW; the
        // lpCreateParams value is the OverlayWindowState pointer passed by this adapter.
        let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        let state = create.lpCreateParams;
        // SAFETY: stores the non-owning state pointer in the HWND for later messages. The owner is
        // the adapter-owned box, which remains stable until after this HWND is destroyed.
        unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, state as isize) };
        return LRESULT(1);
    }

    if message == WM_NCDESTROY {
        // SAFETY: WM_NCDESTROY is the terminal HWND lifetime message. Clear the non-owning pointer
        // before delegating so no later/default-procedure work can observe borrowed state.
        unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, 0) };
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }

    // SAFETY: reads the non-owning state pointer previously installed during WM_NCCREATE.
    let state = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) };
    if state != 0 {
        let state = state as *mut OverlayWindowState;
        // SAFETY: state points into the adapter-owned box for the lifetime of this modal window.
        let state = unsafe { &mut *state };
        match message {
            WM_LBUTTONDOWN => {
                // SAFETY: captures mouse input for this overlay HWND until button release/drop.
                unsafe { SetCapture(window) };
                state.apply(OverlayEvent::DragStart(mouse_point(state.bounds(), lparam)));
                invalidate_overlay(window);
                return LRESULT(0);
            }
            WM_MOUSEMOVE => {
                state.apply(OverlayEvent::DragMove(mouse_point(state.bounds(), lparam)));
                invalidate_overlay(window);
                return LRESULT(0);
            }
            WM_LBUTTONUP => {
                confirm_selection(window, state, mouse_point(state.bounds(), lparam));
                return LRESULT(0);
            }
            WM_KEYDOWN if wparam.0 == usize::from(VK_ESCAPE.0) => {
                state.apply(OverlayEvent::Cancel);
                return LRESULT(0);
            }
            WM_CLOSE => {
                state.apply(OverlayEvent::Cancel);
                return LRESULT(0);
            }
            WM_PAINT => {
                paint_overlay(window, state);
                return LRESULT(0);
            }
            _ => {}
        }
    }

    // SAFETY: messages not handled by this adapter are delegated to the default Win32 procedure.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn invalidate_overlay(window: HWND) {
    // SAFETY: invalidates the full client area for the overlay HWND owned by this adapter.
    let _ = unsafe { InvalidateRect(Some(window), None, true) };
}

fn confirm_selection(window: HWND, state: &mut OverlayWindowState, point: PhysicalPoint) {
    state.apply(OverlayEvent::DragMove(point));
    state.apply(OverlayEvent::Confirm);
    // SAFETY: hides only the adapter-owned overlay HWND. The owner flushes DWM after leaving the
    // modal loop, before capture can begin.
    let _ = unsafe { ShowWindow(window, SW_HIDE) };
    // SAFETY: releases capture taken by this adapter; harmless if capture has already moved.
    let _ = unsafe { ReleaseCapture() };
}

fn paint_overlay(window: HWND, state: &OverlayWindowState) {
    let mut paint = PAINTSTRUCT::default();
    // SAFETY: BeginPaint/EndPaint are paired for WM_PAINT on this overlay HWND.
    let hdc = unsafe { BeginPaint(window, &mut paint) };
    if let Some(rect) = state.preview_client_rect() {
        // SAFETY: stock brush handle is process-global and valid for FrameRect during this call.
        let brush = HBRUSH(unsafe { GetStockObject(WHITE_BRUSH).0 });
        // SAFETY: hdc and rect are valid for the synchronous paint call.
        let _ = unsafe { FrameRect(hdc, &rect, brush) };
    }
    // SAFETY: completes the paint cycle started above.
    let _ = unsafe { EndPaint(window, &paint) };
}
