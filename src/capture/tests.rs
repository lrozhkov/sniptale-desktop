use super::coordinates::{
    CoordinateError, PhysicalPoint, PhysicalRect, normalize_drag_selection, source_local_point,
};
use super::overlay::{OverlayCancelReason, OverlayEvent, OverlayOutcome, RegionOverlayState};
#[cfg(not(windows))]
use super::overlay::{RegionOverlayAvailability, runtime_region_overlay_availability};
use super::png::{BgraFrame, BgraFrameError};
use super::screenshot::{
    CapturedScreenshotFrame, ScreenshotCaptureError, ScreenshotCaptureRunner,
    ScreenshotCaptureService, ScreenshotFrameProvider, ScreenshotPngWriter,
    composite::{composite_bgra_frames, crop_bgra_frame},
    region::{ScreenshotFrameRequest, ScreenshotRegionSelector},
};
use super::screenshot::{ScreenshotPlanError, plan_screenshot, validate_raw_frame_bounds};
use super::sources::{
    CaptureSourceMode, MonitorSnapshot, ResolvedSourceKind, SourceResolveContext,
    SourceResolveError, WindowCaptureState, WindowSnapshot, region_capture_bounds, resolve_source,
};
use super::transfer_messages::{
    ScreenshotTransferMessageError, ScreenshotTransferSessionInput, screenshot_chunk_message,
    screenshot_commit_message, screenshot_start_message, screenshot_transfer_session,
};
use super::transfer_record::FinalizedScreenshotTransfer;
use super::transfer_store::{ScreenshotTransferStore, ScreenshotTransferStoreConfig};
use super::wgc::{WgcPlanError, WgcSourcePolicy};
#[cfg(not(windows))]
use super::wgc::{WgcRuntimeAvailability, runtime_capture_availability};
use super::{ScreenshotCaptureEvent, ScreenshotCaptureRequest};
use crate::protocol::{
    DeliveryRoute, OperationAuthority, ScreenshotMode, WireMessage, parse_wire_message,
};
use crate::transfer::error::TransferError;
use crate::transfer::hashing::sha256_hex;
use std::io::{Read, Seek, SeekFrom, Write};

mod geometry;
mod manual_route;
mod source_plan;
mod transfer_service;

fn monitor(id: &str, x: i32, y: i32, is_primary: bool) -> MonitorSnapshot {
    MonitorSnapshot {
        id: id.to_owned(),
        bounds: PhysicalRect {
            x,
            y,
            width: 1920,
            height: 1080,
        },
        is_primary,
    }
}

fn source_context() -> SourceResolveContext {
    SourceResolveContext {
        monitors: vec![
            monitor("left", -1920, 0, false),
            monitor("primary", 0, 0, true),
        ],
        cursor: Some(PhysicalPoint { x: -100, y: 40 }),
        active_window: Some(WindowSnapshot {
            bounds: PhysicalRect {
                x: 100,
                y: 100,
                width: 800,
                height: 600,
            },
            capture_state: WindowCaptureState::Capturable,
        }),
        region: Some(PhysicalRect {
            x: -100,
            y: 50,
            width: 300,
            height: 200,
        }),
    }
}

fn write_pending_screenshot(
    store: &ScreenshotTransferStore,
    root: &std::path::Path,
    capture_id: &str,
    now_ms: u64,
) -> Result<(), String> {
    store
        .write_finalized_capture_at(
            FinalizedScreenshotTransfer {
                capture_id: capture_id.to_owned(),
                mode: ScreenshotMode::Screen,
                filename: format!("{capture_id}.png"),
                bounds: PhysicalRect {
                    x: 0,
                    y: 0,
                    width: 10,
                    height: 10,
                },
                authority: OperationAuthority::ExtensionManaged {
                    settings_revision: "settings-revision-1".to_owned(),
                },
                delivery_route: DeliveryRoute::ExtensionTransfer,
                total_bytes: 11,
                sha256: sha256_hex(b"hello world"),
                media_protection: None,
                open_editor: true,
            },
            now_ms,
        )
        .map_err(|error| format!("{error:?}"))?;
    std::fs::write(root.join(format!("{capture_id}.png")), b"hello world")
        .map_err(|error| error.to_string())
}

fn screenshot_command(command_id: &str) -> ScreenshotCaptureRequest {
    ScreenshotCaptureRequest {
        operation_id: command_id.to_owned(),
        mode: ScreenshotMode::Screen,
        authority: OperationAuthority::ExtensionManaged {
            settings_revision: "settings-revision-1".to_owned(),
        },
        delivery_route: DeliveryRoute::ExtensionTransfer,
        include_cursor: true,
        open_editor: true,
        requested_at_epoch_ms: 4_000,
    }
}

fn captured_frame(bounds: PhysicalRect, bgra_pixels: &[u8]) -> CapturedScreenshotFrame {
    CapturedScreenshotFrame {
        bounds,
        width: bounds.width,
        height: bounds.height,
        stride: bounds.width * 4,
        bgra_pixels: bgra_pixels.to_vec(),
    }
}

#[derive(Clone, Copy, Debug)]
struct FakeFrameProvider;

impl ScreenshotFrameProvider for FakeFrameProvider {
    fn capture_frame(
        &mut self,
        _request: ScreenshotFrameRequest<'_>,
    ) -> Result<CapturedScreenshotFrame, ScreenshotCaptureError> {
        Ok(CapturedScreenshotFrame {
            bounds: PhysicalRect {
                x: 0,
                y: 0,
                width: 2,
                height: 2,
            },
            width: 2,
            height: 2,
            stride: 8,
            bgra_pixels: vec![0; 16],
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct RegionAwareFrameProvider;

impl ScreenshotFrameProvider for RegionAwareFrameProvider {
    fn capture_frame(
        &mut self,
        request: ScreenshotFrameRequest<'_>,
    ) -> Result<CapturedScreenshotFrame, ScreenshotCaptureError> {
        let region = request
            .selected_region
            .ok_or(ScreenshotCaptureError::UnsupportedMode)?;
        let pixel_bytes = region
            .width
            .checked_mul(region.height)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(ScreenshotCaptureError::RawFrameTooLarge)?;
        let pixel_len =
            usize::try_from(pixel_bytes).map_err(|_| ScreenshotCaptureError::RawFrameTooLarge)?;
        Ok(CapturedScreenshotFrame {
            bounds: region,
            width: region.width,
            height: region.height,
            stride: region.width * 4,
            bgra_pixels: vec![0; pixel_len],
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct FakeRegionSelector {
    region: PhysicalRect,
}

impl ScreenshotRegionSelector for FakeRegionSelector {
    fn select_region(
        &mut self,
        _request: &ScreenshotCaptureRequest,
    ) -> Result<PhysicalRect, ScreenshotCaptureError> {
        Ok(self.region)
    }
}

#[derive(Clone, Debug)]
struct FakePngWriter {
    bytes: Vec<u8>,
}

impl ScreenshotPngWriter for FakePngWriter {
    fn write_png(
        &mut self,
        _frame: &CapturedScreenshotFrame,
        path: &std::path::Path,
    ) -> Result<(), ScreenshotCaptureError> {
        std::fs::write(path, &self.bytes).map_err(|_| ScreenshotCaptureError::FileIo)
    }
}

fn write_writing_screenshot(
    root: &std::path::Path,
    capture_id: &str,
    updated_at_ms: u64,
) -> Result<(), String> {
    let payload = format!(
        r#"{{
            "captureId":"{capture_id}",
            "state":"Writing",
            "createdAtMs":1000,
            "updatedAtMs":{updated_at_ms},
            "mode":"screen",
            "filename":"{capture_id}.png",
            "x":0,
            "y":0,
            "width":10,
            "height":10,
            "totalBytes":null,
            "sha256":null,
            "openEditor":false,
            "retainRejectedUntilMs":null
        }}"#
    );
    std::fs::write(root.join(format!("{capture_id}.json")), payload)
        .map_err(|error| error.to_string())
}

fn write_legacy_finalized_screenshot_metadata(
    root: &std::path::Path,
    capture_id: &str,
    updated_at_ms: u64,
) -> Result<(), String> {
    let payload = format!(
        r#"{{
            "captureId":"{capture_id}",
            "state":"Finalized",
            "createdAtMs":1000,
            "updatedAtMs":{updated_at_ms},
            "mode":"screen",
            "filename":"{capture_id}.png",
            "x":0,
            "y":0,
            "width":10,
            "height":10,
            "totalBytes":11,
            "sha256":"{}",
            "openEditor":false,
            "retainRejectedUntilMs":null
        }}"#,
        sha256_hex(b"hello world")
    );
    std::fs::write(root.join(format!("{capture_id}.json")), payload)
        .map_err(|error| error.to_string())
}

fn read_test_file(path: &std::path::Path) -> Result<Vec<u8>, String> {
    let mut file = std::fs::File::open(path).map_err(|error| error.to_string())?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}

fn tamper_last_byte(path: &std::path::Path) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| error.to_string())?;
    file.seek(SeekFrom::End(-1))
        .map_err(|error| error.to_string())?;
    let mut byte = [0_u8; 1];
    file.read_exact(&mut byte)
        .map_err(|error| error.to_string())?;
    file.seek(SeekFrom::End(-1))
        .map_err(|error| error.to_string())?;
    file.write_all(&[byte[0] ^ 0xff])
        .map_err(|error| error.to_string())
}
