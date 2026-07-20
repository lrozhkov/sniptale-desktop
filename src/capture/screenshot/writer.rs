//! Screenshot PNG writer adapter.

#[cfg(windows)]
use std::path::Path;

#[cfg(windows)]
use super::{CapturedScreenshotFrame, ScreenshotCaptureError, ScreenshotPngWriter};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg(windows)]
pub(crate) struct WindowsWicPngWriter;

#[cfg(windows)]
impl ScreenshotPngWriter for WindowsWicPngWriter {
    fn write_png(
        &mut self,
        frame: &CapturedScreenshotFrame,
        path: &Path,
    ) -> Result<(), ScreenshotCaptureError> {
        let bgra = super::super::png::BgraFrame::new(
            frame.width,
            frame.height,
            frame.stride,
            &frame.bgra_pixels,
        )
        .map_err(|_| ScreenshotCaptureError::InvalidFrame)?;
        super::super::png::encode_bgra_png_file(bgra, path)
            .map_err(|_| ScreenshotCaptureError::PngEncodeFailed)
    }
}
