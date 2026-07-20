//! Source resolution, WGC frame capture, screenshots, overlays, and coordinates.

#[cfg(any(windows, test))]
mod coordinates;
mod overlay;
#[cfg(any(windows, test))]
mod png;
#[cfg(any(windows, test))]
mod screenshot;
#[cfg(any(windows, test))]
mod service_contract;
#[cfg(any(windows, test))]
mod sources;
#[cfg(test)]
mod tests;
#[cfg(any(windows, test))]
mod transfer_disk;
#[cfg(test)]
mod transfer_messages;
#[cfg(any(windows, test))]
mod transfer_record;
#[cfg(any(windows, test))]
mod transfer_store;
mod wgc;

pub(crate) use overlay::{RegionOverlayAvailability, runtime_region_overlay_availability};
#[cfg(windows)]
pub(crate) use screenshot::default_screenshot_capture_runner;
#[cfg(any(windows, test))]
pub(crate) use service_contract::{
    ScreenshotCaptureCompletion, ScreenshotCaptureError, ScreenshotCaptureEvent,
    ScreenshotCaptureRequest, ScreenshotCaptureRunner,
};
#[cfg(any(windows, test))]
pub(crate) use transfer_store::{
    PendingScreenshotTransfer, ScreenshotTransferStore, ScreenshotTransferStoreError,
};
#[cfg(windows)]
pub(crate) use wgc::recording_source_bounds;
#[cfg(windows)]
pub(crate) use wgc::{WgcRecordingFrameError, WindowsWgcRecordingSession};
pub(crate) use wgc::{WgcRuntimeAvailability, runtime_capture_availability};
