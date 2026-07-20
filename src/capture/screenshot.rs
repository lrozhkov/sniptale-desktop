//! Still screenshot capture orchestration.

pub(crate) mod composite;
pub(crate) mod region;
mod writer;

use std::path::Path;

use self::region::{ScreenshotFrameRequest, ScreenshotRegionSelector, UnsupportedRegionSelector};
#[cfg(windows)]
use self::writer::WindowsWicPngWriter;
#[cfg(windows)]
use super::overlay::WindowsRegionSelector;
#[cfg(test)]
use super::sources::{ResolvedCaptureSource, ResolvedSourceKind};
use super::transfer_record::{FinalizedScreenshotTransfer, PreparedScreenshotTransfer};
use super::transfer_store::{ScreenshotTransferStore, ScreenshotTransferStoreError};
#[cfg(test)]
use super::wgc::{WgcFramePlan, WgcPlanError, WgcSourcePolicy, plan_frame_capture};
use super::{ScreenshotCaptureCompletion, ScreenshotCaptureEvent, ScreenshotCaptureRequest};
pub(crate) use super::{ScreenshotCaptureError, ScreenshotCaptureRunner};
use super::{coordinates::PhysicalRect, png::BgraFrameError};
use crate::protocol::{
    DeliveryRoute, OperationAuthority, SCREENSHOT_MAX_TOTAL_BYTES, ScreenshotMode,
};
use crate::transfer::{
    chunks::chunk_count,
    identity::{TransferMediaFileKind, derived_media_id, media_filename},
};

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScreenshotPlanError {
    Frame(WgcPlanError),
    EncodedSizeLimitExceeded,
}

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ScreenshotPlan {
    pub(crate) frame: WgcFramePlan,
    pub(crate) requires_freeze_overlay: bool,
    pub(crate) max_encoded_bytes: u64,
}

#[cfg(test)]
pub(crate) fn plan_screenshot(
    source: ResolvedCaptureSource,
    policy: WgcSourcePolicy,
    max_encoded_bytes: u64,
) -> Result<ScreenshotPlan, ScreenshotPlanError> {
    if max_encoded_bytes == 0 {
        return Err(ScreenshotPlanError::EncodedSizeLimitExceeded);
    }
    let requires_freeze_overlay = source.kind == ResolvedSourceKind::Region;
    let frame = plan_frame_capture(source, policy).map_err(ScreenshotPlanError::Frame)?;
    Ok(ScreenshotPlan {
        frame,
        requires_freeze_overlay,
        max_encoded_bytes,
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CapturedScreenshotFrame {
    pub(crate) bounds: PhysicalRect,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stride: u32,
    pub(crate) bgra_pixels: Vec<u8>,
}

pub(crate) trait ScreenshotFrameProvider {
    fn capture_frame(
        &mut self,
        request: ScreenshotFrameRequest<'_>,
    ) -> Result<CapturedScreenshotFrame, ScreenshotCaptureError>;
}

pub(crate) trait ScreenshotPngWriter {
    fn write_png(
        &mut self,
        frame: &CapturedScreenshotFrame,
        path: &Path,
    ) -> Result<(), ScreenshotCaptureError>;
}

pub(crate) fn default_screenshot_capture_runner(
    store: ScreenshotTransferStore,
) -> Option<Box<dyn ScreenshotCaptureRunner>> {
    platform_screenshot_capture_runner(store)
}

#[cfg(windows)]
fn platform_screenshot_capture_runner(
    store: ScreenshotTransferStore,
) -> Option<Box<dyn ScreenshotCaptureRunner>> {
    Some(Box::new(
        ScreenshotCaptureService::new_with_region_selector(
            store,
            super::wgc::WindowsWgcFrameProvider::default(),
            WindowsWicPngWriter,
            WindowsRegionSelector,
        ),
    ))
}

#[cfg(not(windows))]
fn platform_screenshot_capture_runner(
    _store: ScreenshotTransferStore,
) -> Option<Box<dyn ScreenshotCaptureRunner>> {
    None
}

#[derive(Debug)]
pub(crate) struct ScreenshotCaptureService<P, W, S = UnsupportedRegionSelector> {
    store: ScreenshotTransferStore,
    provider: P,
    writer: W,
    region_selector: S,
}

impl<P, W> ScreenshotCaptureService<P, W, UnsupportedRegionSelector> {
    #[cfg(test)]
    pub(crate) const fn new(store: ScreenshotTransferStore, provider: P, writer: W) -> Self {
        Self {
            store,
            provider,
            writer,
            region_selector: UnsupportedRegionSelector,
        }
    }
}

impl<P, W, S> ScreenshotCaptureService<P, W, S> {
    pub(crate) const fn new_with_region_selector(
        store: ScreenshotTransferStore,
        provider: P,
        writer: W,
        region_selector: S,
    ) -> Self {
        Self {
            store,
            provider,
            writer,
            region_selector,
        }
    }
}

impl<P, W, S> ScreenshotCaptureRunner for ScreenshotCaptureService<P, W, S>
where
    P: ScreenshotFrameProvider,
    W: ScreenshotPngWriter,
    S: ScreenshotRegionSelector,
{
    fn capture_screenshot(
        &mut self,
        request: &ScreenshotCaptureRequest,
    ) -> Result<Vec<ScreenshotCaptureEvent>, ScreenshotCaptureError> {
        if !matches!(
            (&request.authority, &request.delivery_route),
            (
                OperationAuthority::ExtensionManaged { .. },
                DeliveryRoute::ExtensionTransfer
            ) | (
                OperationAuthority::Manual { .. },
                DeliveryRoute::LocalAsset { .. }
            )
        ) {
            return Err(ScreenshotCaptureError::UnsupportedDeliveryRoute);
        }
        let selected_region = if request.mode == ScreenshotMode::Region {
            Some(self.region_selector.select_region(request)?)
        } else {
            None
        };
        let frame = self.provider.capture_frame(ScreenshotFrameRequest {
            request,
            selected_region,
        })?;
        validate_raw_frame_bounds(frame.width, frame.height, frame.stride)?;
        let capture_id = derived_media_id("capture", &request.operation_id);
        let filename = media_filename(&capture_id, TransferMediaFileKind::Png)
            .map_err(|_| ScreenshotCaptureError::TransferState)?;
        let media_path = self.store.screenshot_media_path(&capture_id)?;
        self.store.start_screenshot_at(
            PreparedScreenshotTransfer {
                capture_id: capture_id.clone(),
                mode: request.mode.clone(),
                filename: filename.clone(),
                bounds: frame.bounds,
                authority: request.authority.clone(),
                delivery_route: request.delivery_route.clone(),
                open_editor: request.open_editor,
            },
            request.requested_at_epoch_ms,
        )?;
        if let Err(error) = self.writer.write_png(&frame, &media_path) {
            let _ = self.store.discard_screenshot(&capture_id);
            return Err(error);
        }
        let result = self.finalized_events(request, &frame, &capture_id, filename);
        if result.is_err() {
            let _ = self.store.discard_screenshot(&capture_id);
        }
        result
    }
}

pub(super) fn validate_raw_frame_bounds(
    width: u32,
    height: u32,
    stride: u32,
) -> Result<(), ScreenshotCaptureError> {
    let row_bytes = width
        .checked_mul(4)
        .ok_or(ScreenshotCaptureError::RawFrameTooLarge)?;
    if width == 0 || height == 0 || stride < row_bytes {
        return Err(ScreenshotCaptureError::RawFrameTooLarge);
    }
    let frame_bytes = u64::from(stride)
        .checked_mul(u64::from(height))
        .ok_or(ScreenshotCaptureError::RawFrameTooLarge)?;
    if frame_bytes > SCREENSHOT_MAX_TOTAL_BYTES {
        Err(ScreenshotCaptureError::RawFrameTooLarge)
    } else {
        Ok(())
    }
}

impl<P, W, S> ScreenshotCaptureService<P, W, S>
where
    P: ScreenshotFrameProvider,
    W: ScreenshotPngWriter,
    S: ScreenshotRegionSelector,
{
    fn finalized_events(
        &self,
        request: &ScreenshotCaptureRequest,
        frame: &CapturedScreenshotFrame,
        capture_id: &str,
        filename: String,
    ) -> Result<Vec<ScreenshotCaptureEvent>, ScreenshotCaptureError> {
        let (total_bytes, sha256, media_protection) = self
            .store
            .protect_screenshot_media(capture_id)
            .map_err(|_| ScreenshotCaptureError::FileIo)?;
        self.store.finalize_prepared_screenshot_at(
            FinalizedScreenshotTransfer {
                capture_id: capture_id.to_owned(),
                mode: request.mode.clone(),
                filename: filename.clone(),
                bounds: frame.bounds,
                authority: request.authority.clone(),
                delivery_route: request.delivery_route.clone(),
                total_bytes,
                sha256: sha256.clone(),
                media_protection: Some(media_protection),
                open_editor: request.open_editor,
            },
            request.requested_at_epoch_ms,
        )?;
        let chunk_count = chunk_count(total_bytes)
            .map_err(|_| ScreenshotCaptureError::TransferState)
            .and_then(|count| {
                u32::try_from(count).map_err(|_| ScreenshotCaptureError::TransferState)
            })?;
        Ok(vec![ScreenshotCaptureEvent::Completed(
            ScreenshotCaptureCompletion {
                authority: request.authority.clone(),
                delivery_route: request.delivery_route.clone(),
                capture_id: capture_id.to_owned(),
                mode: request.mode.clone(),
                filename,
                width: frame.width,
                height: frame.height,
                total_bytes,
                sha256,
                chunk_count,
                open_editor: request.open_editor,
                captured_at_epoch_ms: request.requested_at_epoch_ms,
            },
        )])
    }
}

impl From<ScreenshotTransferStoreError> for ScreenshotCaptureError {
    fn from(_error: ScreenshotTransferStoreError) -> Self {
        Self::TransferState
    }
}

impl From<BgraFrameError> for ScreenshotCaptureError {
    fn from(_error: BgraFrameError) -> Self {
        Self::InvalidFrame
    }
}
