//! Platform-neutral screenshot execution contract.

use crate::protocol::{DeliveryRoute, OperationAuthority, ScreenshotMode};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScreenshotCaptureRequest {
    pub(crate) operation_id: String,
    pub(crate) mode: ScreenshotMode,
    pub(crate) authority: OperationAuthority,
    pub(crate) delivery_route: DeliveryRoute,
    pub(crate) include_cursor: bool,
    pub(crate) open_editor: bool,
    pub(crate) requested_at_epoch_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScreenshotCaptureCompletion {
    pub(crate) authority: OperationAuthority,
    pub(crate) delivery_route: DeliveryRoute,
    pub(crate) capture_id: String,
    pub(crate) mode: ScreenshotMode,
    pub(crate) filename: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) total_bytes: u64,
    pub(crate) chunk_count: u32,
    pub(crate) sha256: String,
    pub(crate) open_editor: bool,
    pub(crate) captured_at_epoch_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ScreenshotCaptureEvent {
    Completed(ScreenshotCaptureCompletion),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScreenshotCaptureError {
    CaptureUnavailable,
    UnsupportedMode,
    UnsupportedDeliveryRoute,
    #[cfg(windows)]
    RegionSelectionCancelled,
    #[cfg(windows)]
    InvalidSelection,
    RawFrameTooLarge,
    InvalidFrame,
    #[cfg(windows)]
    PngEncodeFailed,
    TransferState,
    FileIo,
}

pub(crate) trait ScreenshotCaptureRunner {
    fn capture_screenshot(
        &mut self,
        request: &ScreenshotCaptureRequest,
    ) -> Result<Vec<ScreenshotCaptureEvent>, ScreenshotCaptureError>;
}
