//! Sink Writer sample validation.
use super::{AacSinkWriterStreamPlan, H264SinkWriterStreamPlan};

pub(crate) const PCM_BITS_PER_SAMPLE: u32 = 16;
pub(crate) const PCM_BYTES_PER_SAMPLE: u32 = PCM_BITS_PER_SAMPLE / 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SinkWriterSampleError {
    InvalidTime,
    InvalidVideoGeometry,
    VideoFrameTooLarge,
    InvalidAudioBytes,
    AudioSampleTooLarge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BgraVideoSample<'a> {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) stride: u32,
    pub(crate) bgra_bytes: &'a [u8],
    pub(crate) timestamp_100ns: i64,
    pub(crate) duration_100ns: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PcmAudioSample<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) timestamp_100ns: i64,
    pub(crate) duration_100ns: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BgraVideoSampleLayout {
    pub(crate) row_bytes: usize,
    pub(crate) source_span: usize,
    pub(crate) packed_len: usize,
}

pub(crate) fn validate_bgra_video_sample(
    plan: &H264SinkWriterStreamPlan,
    sample: BgraVideoSample<'_>,
) -> Result<BgraVideoSampleLayout, SinkWriterSampleError> {
    validate_sample_time(sample.timestamp_100ns, sample.duration_100ns)?;
    if sample.width != plan.width || sample.height != plan.height {
        return Err(SinkWriterSampleError::InvalidVideoGeometry);
    }
    let row_bytes = sample
        .width
        .checked_mul(4)
        .ok_or(SinkWriterSampleError::VideoFrameTooLarge)?;
    if row_bytes == 0 || sample.stride < row_bytes {
        return Err(SinkWriterSampleError::InvalidVideoGeometry);
    }
    let source_bytes = u64::from(sample.stride)
        .checked_mul(u64::from(sample.height))
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(SinkWriterSampleError::VideoFrameTooLarge)?;
    let packed_bytes = u64::from(row_bytes)
        .checked_mul(u64::from(sample.height))
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(SinkWriterSampleError::VideoFrameTooLarge)?;
    if sample.bgra_bytes.len() < source_bytes {
        return Err(SinkWriterSampleError::VideoFrameTooLarge);
    }
    Ok(BgraVideoSampleLayout {
        row_bytes: usize::try_from(row_bytes)
            .map_err(|_| SinkWriterSampleError::VideoFrameTooLarge)?,
        source_span: source_bytes,
        packed_len: packed_bytes,
    })
}

pub(crate) fn validate_pcm_audio_sample(
    plan: &AacSinkWriterStreamPlan,
    sample: PcmAudioSample<'_>,
) -> Result<usize, SinkWriterSampleError> {
    validate_sample_time(sample.timestamp_100ns, sample.duration_100ns)?;
    if sample.bytes.is_empty() {
        return Err(SinkWriterSampleError::InvalidAudioBytes);
    }
    let block_alignment = u32::from(plan.channels)
        .checked_mul(PCM_BYTES_PER_SAMPLE)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(SinkWriterSampleError::AudioSampleTooLarge)?;
    if block_alignment == 0 || !sample.bytes.len().is_multiple_of(block_alignment) {
        return Err(SinkWriterSampleError::InvalidAudioBytes);
    }
    u32::try_from(sample.bytes.len()).map_err(|_| SinkWriterSampleError::AudioSampleTooLarge)?;
    Ok(block_alignment)
}

fn validate_sample_time(
    timestamp_100ns: i64,
    duration_100ns: i64,
) -> Result<(), SinkWriterSampleError> {
    if timestamp_100ns < 0 || duration_100ns <= 0 {
        Err(SinkWriterSampleError::InvalidTime)
    } else {
        Ok(())
    }
}
