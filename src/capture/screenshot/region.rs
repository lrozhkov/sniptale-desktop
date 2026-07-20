//! Region selection request seam.

use super::super::coordinates::PhysicalRect;
use super::{ScreenshotCaptureError, ScreenshotCaptureRequest};

#[derive(Clone, Copy, Debug)]
pub(crate) struct ScreenshotFrameRequest<'a> {
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) request: &'a ScreenshotCaptureRequest,
    #[cfg_attr(not(windows), allow(dead_code))]
    pub(crate) selected_region: Option<PhysicalRect>,
}

pub(crate) trait ScreenshotRegionSelector {
    fn select_region(
        &mut self,
        request: &ScreenshotCaptureRequest,
    ) -> Result<PhysicalRect, ScreenshotCaptureError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct UnsupportedRegionSelector;

impl ScreenshotRegionSelector for UnsupportedRegionSelector {
    fn select_region(
        &mut self,
        _request: &ScreenshotCaptureRequest,
    ) -> Result<PhysicalRect, ScreenshotCaptureError> {
        Err(ScreenshotCaptureError::UnsupportedMode)
    }
}
