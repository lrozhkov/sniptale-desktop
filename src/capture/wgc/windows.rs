//! Windows Graphics Capture frame provider.

mod apartment;
#[cfg(test)]
mod live_tests;
mod readback;
mod source;

use super::super::screenshot::{
    CapturedScreenshotFrame, ScreenshotCaptureError, ScreenshotFrameProvider,
    composite::{composite_bgra_frames, crop_bgra_frame},
    region::ScreenshotFrameRequest,
};
use super::recording_owner::RecordingCaptureOwners;
use crate::protocol::NativeRecordingSource;

pub(crate) use source::recording_source_bounds;

#[derive(Debug)]
pub(crate) struct CapturedRecordingFrame {
    pub(crate) frame: CapturedScreenshotFrame,
    pub(crate) system_relative_time_100ns: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WgcRecordingFrameError {
    State,
    Poll(i32),
    Timestamp(i32),
    Surface(i32),
    Interface(i32),
    Readback,
    Composite,
}

#[derive(Debug)]
struct ActiveRecordingTarget {
    bounds: super::super::coordinates::PhysicalRect,
    session: readback::CaptureSession,
    pending_frame: Option<readback::ReadbackFrame>,
}

#[derive(Debug)]
pub(crate) struct WindowsWgcRecordingSession {
    owners: RecordingCaptureOwners<
        Vec<ActiveRecordingTarget>,
        readback::D3dDevice,
        apartment::WinrtApartment,
    >,
    capture_bounds: super::super::coordinates::PhysicalRect,
    output_bounds: super::super::coordinates::PhysicalRect,
}

impl WindowsWgcRecordingSession {
    pub(crate) fn start(
        source: &NativeRecordingSource,
        include_cursor: bool,
    ) -> Result<Self, ScreenshotCaptureError> {
        let winrt = apartment::WinrtApartment::initialize()?;
        let device = readback::D3dDevice::new()?;
        let targets = source::recording_targets(source)?;
        if !readback::request_borderless_access() {
            return Err(ScreenshotCaptureError::CaptureUnavailable);
        }
        let active_targets = targets
            .items
            .iter()
            .map(|target| {
                let item = target.create_item()?;
                Ok(ActiveRecordingTarget {
                    bounds: target.bounds,
                    session: readback::CaptureSession::start(
                        &device,
                        &item,
                        2,
                        true,
                        include_cursor,
                    )?,
                    pending_frame: None,
                })
            })
            .collect::<Result<Vec<_>, ScreenshotCaptureError>>()?;
        Ok(Self {
            owners: RecordingCaptureOwners::new(active_targets, device, winrt),
            capture_bounds: targets.capture_bounds,
            output_bounds: targets.output_bounds,
        })
    }

    pub(crate) fn try_next_frame(
        &mut self,
    ) -> Result<Option<CapturedRecordingFrame>, WgcRecordingFrameError> {
        let mut timestamp = None;
        let (targets, device) = self
            .owners
            .sessions_and_device()
            .ok_or(WgcRecordingFrameError::State)?;
        for target in targets.iter_mut() {
            if let Some(frame) = target
                .session
                .try_next_recording_frame(device)
                .map_err(map_recording_frame_error)?
            {
                target.pending_frame = Some(frame);
            }
        }
        if targets.iter().any(|target| target.pending_frame.is_none()) {
            return Ok(None);
        }
        let mut frames = Vec::with_capacity(targets.len());
        for target in targets.iter_mut() {
            let frame = target
                .pending_frame
                .take()
                .ok_or(WgcRecordingFrameError::State)?;
            timestamp = Some(
                timestamp.map_or(frame.system_relative_time_100ns, |current: i64| {
                    current.max(frame.system_relative_time_100ns)
                }),
            );
            frames.push(CapturedScreenshotFrame {
                bounds: target.bounds,
                width: frame.width,
                height: frame.height,
                stride: frame.stride,
                bgra_pixels: frame.bgra_pixels,
            });
        }
        Ok(Some(CapturedRecordingFrame {
            frame: finish_targets_frame(frames, self.capture_bounds, self.output_bounds)
                .map_err(|_| WgcRecordingFrameError::Composite)?,
            system_relative_time_100ns: timestamp.ok_or(WgcRecordingFrameError::State)?,
        }))
    }
}

const fn map_recording_frame_error(
    error: readback::RecordingFrameReadError,
) -> WgcRecordingFrameError {
    match error {
        readback::RecordingFrameReadError::Poll(code) => WgcRecordingFrameError::Poll(code),
        readback::RecordingFrameReadError::Timestamp(code) => {
            WgcRecordingFrameError::Timestamp(code)
        }
        readback::RecordingFrameReadError::Surface(code) => WgcRecordingFrameError::Surface(code),
        readback::RecordingFrameReadError::Interface(code) => {
            WgcRecordingFrameError::Interface(code)
        }
        readback::RecordingFrameReadError::Readback => WgcRecordingFrameError::Readback,
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct WindowsWgcFrameProvider;

impl ScreenshotFrameProvider for WindowsWgcFrameProvider {
    fn capture_frame(
        &mut self,
        request: ScreenshotFrameRequest<'_>,
    ) -> Result<CapturedScreenshotFrame, ScreenshotCaptureError> {
        let targets =
            source::CaptureTargets::resolve(request.request.mode.clone(), request.selected_region)?;
        capture_targets_frame(targets, request.request.include_cursor)
    }
}

fn capture_targets_frame(
    targets: source::CaptureTargets,
    include_cursor: bool,
) -> Result<CapturedScreenshotFrame, ScreenshotCaptureError> {
    let _winrt = apartment::WinrtApartment::initialize()?;
    let device = readback::D3dDevice::new()?;
    let mut frames = Vec::with_capacity(targets.items.len());
    for target in &targets.items {
        let item = target.create_item()?;
        let frame = readback::capture_item_frame(&device, &item, include_cursor)?;
        frames.push(CapturedScreenshotFrame {
            bounds: target.bounds,
            width: frame.width,
            height: frame.height,
            stride: frame.stride,
            bgra_pixels: frame.bgra_pixels,
        });
    }
    finish_targets_frame(frames, targets.capture_bounds, targets.output_bounds)
}

fn finish_targets_frame(
    mut frames: Vec<CapturedScreenshotFrame>,
    capture_bounds: super::super::coordinates::PhysicalRect,
    output_bounds: super::super::coordinates::PhysicalRect,
) -> Result<CapturedScreenshotFrame, ScreenshotCaptureError> {
    let frame = if frames.len() == 1 {
        frames
            .pop()
            .ok_or(ScreenshotCaptureError::CaptureUnavailable)
    } else {
        composite_bgra_frames(&frames, capture_bounds)
    }?;
    if frame.bounds == output_bounds {
        Ok(frame)
    } else {
        crop_bgra_frame(&frame, output_bounds)
    }
}
