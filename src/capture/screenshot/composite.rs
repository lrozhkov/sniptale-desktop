//! BGRA frame compositing for multi-source captures.

use super::super::coordinates::PhysicalRect;
use super::{CapturedScreenshotFrame, ScreenshotCaptureError, validate_raw_frame_bounds};

pub(crate) fn composite_bgra_frames(
    frames: &[CapturedScreenshotFrame],
    bounds: PhysicalRect,
) -> Result<CapturedScreenshotFrame, ScreenshotCaptureError> {
    if frames.is_empty() {
        return Err(ScreenshotCaptureError::CaptureUnavailable);
    }
    validate_raw_frame_bounds(bounds.width, bounds.height, output_stride(bounds.width)?)?;
    let stride = output_stride(bounds.width)?;
    let output_len = frame_len(stride, bounds.height)?;
    let mut bgra_pixels = vec![0; output_len];

    for frame in frames {
        validate_frame_for_bounds(frame, bounds)?;
        copy_frame(frame, bounds, stride, &mut bgra_pixels)?;
    }

    Ok(CapturedScreenshotFrame {
        bounds,
        width: bounds.width,
        height: bounds.height,
        stride,
        bgra_pixels,
    })
}

pub(crate) fn crop_bgra_frame(
    frame: &CapturedScreenshotFrame,
    bounds: PhysicalRect,
) -> Result<CapturedScreenshotFrame, ScreenshotCaptureError> {
    validate_frame_for_bounds(frame, frame.bounds)?;
    if !frame.bounds.contains_rect(bounds) {
        return Err(ScreenshotCaptureError::CaptureUnavailable);
    }
    validate_raw_frame_bounds(bounds.width, bounds.height, output_stride(bounds.width)?)?;
    let stride = output_stride(bounds.width)?;
    let output_len = frame_len(stride, bounds.height)?;
    let mut bgra_pixels = vec![0; output_len];

    let source_stride =
        usize::try_from(frame.stride).map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    let target_stride =
        usize::try_from(stride).map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    let src_x = usize::try_from(bounds.x - frame.bounds.x)
        .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    let src_y = usize::try_from(bounds.y - frame.bounds.y)
        .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    let src_x_bytes = src_x
        .checked_mul(4)
        .ok_or(ScreenshotCaptureError::CaptureUnavailable)?;
    let row_bytes = usize::try_from(output_stride(bounds.width)?)
        .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    let height =
        usize::try_from(bounds.height).map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;

    for row in 0..height {
        let src_start = src_y
            .checked_add(row)
            .and_then(|row| row.checked_mul(source_stride))
            .and_then(|offset| offset.checked_add(src_x_bytes))
            .ok_or(ScreenshotCaptureError::CaptureUnavailable)?;
        let dst_start = row
            .checked_mul(target_stride)
            .ok_or(ScreenshotCaptureError::CaptureUnavailable)?;
        let src_end = src_start
            .checked_add(row_bytes)
            .ok_or(ScreenshotCaptureError::CaptureUnavailable)?;
        let dst_end = dst_start
            .checked_add(row_bytes)
            .ok_or(ScreenshotCaptureError::CaptureUnavailable)?;
        bgra_pixels[dst_start..dst_end].copy_from_slice(&frame.bgra_pixels[src_start..src_end]);
    }

    Ok(CapturedScreenshotFrame {
        bounds,
        width: bounds.width,
        height: bounds.height,
        stride,
        bgra_pixels,
    })
}

fn validate_frame_for_bounds(
    frame: &CapturedScreenshotFrame,
    bounds: PhysicalRect,
) -> Result<(), ScreenshotCaptureError> {
    if frame.width != frame.bounds.width
        || frame.height != frame.bounds.height
        || !bounds.contains_rect(frame.bounds)
    {
        return Err(ScreenshotCaptureError::CaptureUnavailable);
    }
    validate_raw_frame_bounds(frame.width, frame.height, frame.stride)?;
    let expected = frame_len(frame.stride, frame.height)?;
    if frame.bgra_pixels.len() < expected {
        return Err(ScreenshotCaptureError::CaptureUnavailable);
    }
    Ok(())
}

fn copy_frame(
    frame: &CapturedScreenshotFrame,
    bounds: PhysicalRect,
    target_stride_u32: u32,
    output: &mut [u8],
) -> Result<(), ScreenshotCaptureError> {
    let dst_x = usize::try_from(frame.bounds.x - bounds.x)
        .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    let dst_y = usize::try_from(frame.bounds.y - bounds.y)
        .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    let row_bytes = usize::try_from(output_stride(frame.width)?)
        .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    let source_stride =
        usize::try_from(frame.stride).map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    let target_stride = usize::try_from(target_stride_u32)
        .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    let dst_x_bytes = dst_x
        .checked_mul(4)
        .ok_or(ScreenshotCaptureError::CaptureUnavailable)?;

    let height =
        usize::try_from(frame.height).map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
    for row in 0..height {
        let src_start = row
            .checked_mul(source_stride)
            .ok_or(ScreenshotCaptureError::CaptureUnavailable)?;
        let dst_start = dst_y
            .checked_add(row)
            .and_then(|row| row.checked_mul(target_stride))
            .and_then(|offset| offset.checked_add(dst_x_bytes))
            .ok_or(ScreenshotCaptureError::CaptureUnavailable)?;
        let src_end = src_start
            .checked_add(row_bytes)
            .ok_or(ScreenshotCaptureError::CaptureUnavailable)?;
        let dst_end = dst_start
            .checked_add(row_bytes)
            .ok_or(ScreenshotCaptureError::CaptureUnavailable)?;
        output[dst_start..dst_end].copy_from_slice(&frame.bgra_pixels[src_start..src_end]);
    }
    Ok(())
}

fn output_stride(width: u32) -> Result<u32, ScreenshotCaptureError> {
    width
        .checked_mul(4)
        .ok_or(ScreenshotCaptureError::RawFrameTooLarge)
}

fn frame_len(stride: u32, height: u32) -> Result<usize, ScreenshotCaptureError> {
    usize::try_from(stride)
        .ok()
        .and_then(|stride| {
            usize::try_from(height)
                .ok()
                .and_then(|height| stride.checked_mul(height))
        })
        .ok_or(ScreenshotCaptureError::RawFrameTooLarge)
}
