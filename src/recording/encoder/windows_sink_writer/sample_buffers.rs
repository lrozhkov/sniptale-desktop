//! Media Foundation sample buffer construction for Sink Writer input.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::borrow::Cow;

use windows::Win32::Media::MediaFoundation::{
    IMFMediaBuffer, IMFSample, MFCreateMemoryBuffer, MFCreateSample,
};

use super::{Mp4SinkWriterError, to_error};
use crate::recording::encoder::{
    AacSinkWriterStreamPlan, H264SinkWriterStreamPlan,
    samples::{
        BgraVideoSample, PcmAudioSample, validate_bgra_video_sample, validate_pcm_audio_sample,
    },
};

pub(super) fn bgra_video_bytes<'a>(
    plan: &H264SinkWriterStreamPlan,
    sample: BgraVideoSample<'a>,
) -> Result<Cow<'a, [u8]>, Mp4SinkWriterError> {
    let layout = validate_bgra_video_sample(plan, sample)?;
    if sample.stride
        == u32::try_from(layout.row_bytes).map_err(|_| Mp4SinkWriterError::InvalidCodecValue)?
    {
        return Ok(Cow::Borrowed(&sample.bgra_bytes[..layout.packed_len]));
    }
    let mut packed = Vec::with_capacity(layout.packed_len);
    let stride =
        usize::try_from(sample.stride).map_err(|_| Mp4SinkWriterError::InvalidCodecValue)?;
    let height = usize::try_from(plan.height).map_err(|_| Mp4SinkWriterError::InvalidCodecValue)?;
    for row_index in 0..height {
        let start = row_index
            .checked_mul(stride)
            .ok_or(Mp4SinkWriterError::InvalidCodecValue)?;
        let end = start
            .checked_add(layout.row_bytes)
            .ok_or(Mp4SinkWriterError::InvalidCodecValue)?;
        packed.extend_from_slice(&sample.bgra_bytes[start..end]);
    }
    Ok(Cow::Owned(packed))
}

pub(super) fn validate_pcm_audio_bytes(
    plan: &AacSinkWriterStreamPlan,
    sample: PcmAudioSample<'_>,
) -> Result<(), Mp4SinkWriterError> {
    validate_pcm_audio_sample(plan, sample)?;
    Ok(())
}

pub(super) fn media_sample_from_bytes(
    bytes: &[u8],
    timestamp_100ns: i64,
    duration_100ns: i64,
) -> Result<IMFSample, Mp4SinkWriterError> {
    let byte_len = u32::try_from(bytes.len()).map_err(|_| Mp4SinkWriterError::InvalidCodecValue)?;
    let buffer = memory_buffer(bytes, byte_len)?;
    // SAFETY: creates an empty Media Foundation sample owned by COM RAII.
    let sample = unsafe { MFCreateSample() }.map_err(to_error)?;
    // SAFETY: buffer is a live IMFMediaBuffer containing byte_len initialized bytes.
    unsafe { sample.AddBuffer(&buffer) }.map_err(to_error)?;
    // SAFETY: timestamp and duration were validated by the caller's sample validator.
    unsafe { sample.SetSampleTime(timestamp_100ns) }.map_err(to_error)?;
    // SAFETY: timestamp and duration were validated by the caller's sample validator.
    unsafe { sample.SetSampleDuration(duration_100ns) }.map_err(to_error)?;
    Ok(sample)
}

fn memory_buffer(bytes: &[u8], byte_len: u32) -> Result<IMFMediaBuffer, Mp4SinkWriterError> {
    // SAFETY: creates a memory buffer with exactly byte_len capacity and returns a COM-owned buffer.
    let buffer = unsafe { MFCreateMemoryBuffer(byte_len) }.map_err(to_error)?;
    {
        let lock = LockedMediaBuffer::new(&buffer)?;
        lock.copy_from(bytes)?;
    }
    // SAFETY: byte_len bytes were initialized while the buffer was locked above.
    unsafe { buffer.SetCurrentLength(byte_len) }.map_err(to_error)?;
    Ok(buffer)
}

#[derive(Debug)]
struct LockedMediaBuffer<'a> {
    buffer: &'a IMFMediaBuffer,
    ptr: *mut u8,
    len: usize,
}

impl<'a> LockedMediaBuffer<'a> {
    fn new(buffer: &'a IMFMediaBuffer) -> Result<Self, Mp4SinkWriterError> {
        let mut ptr = std::ptr::null_mut();
        let mut max_len = 0_u32;
        // SAFETY: ptr/max_len are valid output locations and buffer is a live IMFMediaBuffer.
        unsafe { buffer.Lock(&raw mut ptr, Some(&raw mut max_len), None) }.map_err(to_error)?;
        if ptr.is_null() {
            // SAFETY: unlocks the buffer locked immediately above before returning an error.
            let _ = unsafe { buffer.Unlock() };
            return Err(Mp4SinkWriterError::InvalidCodecValue);
        }
        Ok(Self {
            buffer,
            ptr,
            len: usize::try_from(max_len).map_err(|_| Mp4SinkWriterError::InvalidCodecValue)?,
        })
    }

    fn copy_from(&self, bytes: &[u8]) -> Result<(), Mp4SinkWriterError> {
        if bytes.len() > self.len {
            return Err(Mp4SinkWriterError::InvalidCodecValue);
        }
        let destination = self.ptr;
        // SAFETY: destination points to a locked MF buffer with at least self.len bytes. The copy
        // length is bounded by self.len and bytes.len(), and source/destination do not overlap.
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), destination, bytes.len()) };
        Ok(())
    }
}

impl Drop for LockedMediaBuffer<'_> {
    fn drop(&mut self) {
        // SAFETY: this guard is constructed only after a successful Lock call on this buffer.
        let _ = unsafe { self.buffer.Unlock() };
    }
}
