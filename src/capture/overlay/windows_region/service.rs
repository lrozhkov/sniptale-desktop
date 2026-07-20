#![cfg(windows)]

use super::{WindowsRegionOverlay, virtual_desktop_bounds};
use crate::capture::{coordinates::PhysicalRect, screenshot::ScreenshotCaptureError};

pub(super) fn select_region() -> Result<PhysicalRect, ScreenshotCaptureError> {
    let bounds = virtual_desktop_bounds()?;
    let mut overlay = WindowsRegionOverlay::create(bounds)?;
    overlay.run_message_loop()?;
    overlay.dismiss_and_flush()?;
    let state = overlay.close()?;
    (*state).into_result()
}
