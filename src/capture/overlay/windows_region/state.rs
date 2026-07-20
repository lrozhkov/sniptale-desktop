#![cfg(windows)]

use windows::Win32::Foundation::RECT;

use super::super::{
    super::{coordinates::PhysicalRect, screenshot::ScreenshotCaptureError},
    OverlayCancelReason, OverlayEvent, OverlayOutcome, RegionOverlayState,
};

const MIN_SELECTION_SIZE_PX: u32 = 4;

#[derive(Debug)]
pub(super) struct OverlayWindowState {
    bounds: PhysicalRect,
    state: RegionOverlayState,
    outcome: Option<OverlayOutcome>,
}

impl OverlayWindowState {
    pub(super) const fn new(bounds: PhysicalRect) -> Self {
        Self {
            bounds,
            state: RegionOverlayState::new(MIN_SELECTION_SIZE_PX),
            outcome: None,
        }
    }

    pub(super) const fn bounds(&self) -> PhysicalRect {
        self.bounds
    }

    pub(super) const fn has_outcome(&self) -> bool {
        self.outcome.is_some()
    }

    pub(super) fn into_result(self) -> Result<PhysicalRect, ScreenshotCaptureError> {
        match self.outcome {
            Some(OverlayOutcome::Confirmed(rect)) => Ok(rect),
            Some(OverlayOutcome::Cancelled(OverlayCancelReason::UserCancelled)) | None => {
                Err(ScreenshotCaptureError::RegionSelectionCancelled)
            }
            Some(OverlayOutcome::Cancelled(_)) => Err(ScreenshotCaptureError::CaptureUnavailable),
            Some(OverlayOutcome::InvalidSelection | OverlayOutcome::Pending) => {
                Err(ScreenshotCaptureError::InvalidSelection)
            }
        }
    }

    pub(super) fn apply(&mut self, event: OverlayEvent) {
        let outcome = self.state.apply(event);
        if !matches!(outcome, OverlayOutcome::Pending) {
            self.outcome = Some(outcome);
        }
    }

    pub(super) fn preview_client_rect(&self) -> Option<RECT> {
        let rect = self.state.preview_rect()?;
        let left = rect.x.checked_sub(self.bounds.x)?;
        let top = rect.y.checked_sub(self.bounds.y)?;
        let width = i32::try_from(rect.width).ok()?;
        let height = i32::try_from(rect.height).ok()?;
        Some(RECT {
            left,
            top,
            right: left.checked_add(width)?,
            bottom: top.checked_add(height)?,
        })
    }
}
