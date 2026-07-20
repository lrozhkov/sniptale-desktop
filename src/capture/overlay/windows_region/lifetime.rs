//! HWND ownership and borrowed overlay-state lifetime closure.
#![cfg(windows)]
#![allow(unsafe_code)]

use windows::Win32::UI::WindowsAndMessaging::{DestroyWindow, GWLP_USERDATA, SetWindowLongPtrW};

use super::WindowsRegionOverlay;
use crate::capture::screenshot::ScreenshotCaptureError;

impl WindowsRegionOverlay {
    pub(super) fn close(
        mut self,
    ) -> Result<Box<super::state::OverlayWindowState>, ScreenshotCaptureError> {
        self.clear_state_and_destroy()?;
        self.state
            .take()
            .ok_or(ScreenshotCaptureError::CaptureUnavailable)
    }

    fn clear_state_and_destroy(&mut self) -> Result<(), ScreenshotCaptureError> {
        if self.window.is_invalid() {
            return Ok(());
        }
        let window = std::mem::take(&mut self.window);
        // SAFETY: the adapter owns this HWND. Clearing the non-owning state pointer before
        // DestroyWindow makes every synchronous final message independent of the boxed state.
        let previous = unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, 0) };
        // SAFETY: the HWND was moved out of this owner and is destroyed at most once here.
        let destroyed = unsafe { DestroyWindow(window) };
        if previous == 0 && destroyed.is_err() {
            // A live HWND retaining an uncleared borrow cannot outlive the state. Fail closed
            // before Rust can unwind and release that state.
            std::process::abort();
        }
        destroyed.map_err(|_| ScreenshotCaptureError::CaptureUnavailable)
    }
}

impl Drop for WindowsRegionOverlay {
    fn drop(&mut self) {
        let _ = self.clear_state_and_destroy();
    }
}
