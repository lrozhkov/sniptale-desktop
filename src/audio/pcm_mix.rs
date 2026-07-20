//! Platform-neutral PCM chunk remap, resample, and mix owner.
use super::mixer::{AudioInputKind, MixPlan, PcmFormat, validate_format};

const PCM_BYTES_PER_SAMPLE: usize = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PcmMixError {
    InvalidFormat,
    MissingMicrophone,
    MissingSystemAudio,
    WrongInputKind,
    InvalidInputTime,
    InvalidInputBytes,
    UnsyncedInputs,
    OutputTooLarge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PcmInputChunk<'a> {
    pub(crate) kind: AudioInputKind,
    pub(crate) format: PcmFormat,
    pub(crate) bytes: &'a [u8],
    pub(crate) timestamp_100ns: i64,
    pub(crate) duration_100ns: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MixedPcmChunk {
    pub(crate) bytes: Vec<u8>,
    pub(crate) timestamp_100ns: i64,
    pub(crate) duration_100ns: i64,
}

pub(crate) fn mix_pcm_chunks(
    plan: &MixPlan,
    microphone: Option<PcmInputChunk<'_>>,
    system_loopback: Option<PcmInputChunk<'_>>,
) -> Result<MixedPcmChunk, PcmMixError> {
    validate_format(plan.target_format).map_err(|_| PcmMixError::InvalidFormat)?;
    let mut sources = Vec::with_capacity(2);
    if plan.microphone.is_some() {
        sources.push(required_chunk(
            microphone,
            AudioInputKind::Microphone,
            PcmMixError::MissingMicrophone,
        )?);
    }
    if plan.system_loopback.is_some() {
        sources.push(required_chunk(
            system_loopback,
            AudioInputKind::SystemLoopback,
            PcmMixError::MissingSystemAudio,
        )?);
    }
    if sources.is_empty() {
        return Err(PcmMixError::InvalidFormat);
    }

    let (timestamp_100ns, duration_100ns, output_frames) =
        synced_output_shape(&sources, plan.target_format)?;
    let mut mixed_samples = vec![
        0_i32;
        output_frames
            .checked_mul(usize::from(plan.target_format.channels))
            .ok_or(PcmMixError::OutputTooLarge)?
    ];
    for source in &sources {
        accumulate_source(
            &mut mixed_samples,
            plan.target_format,
            *source,
            output_frames,
        )?;
    }

    let source_count = i32::try_from(sources.len()).map_err(|_| PcmMixError::OutputTooLarge)?;
    let mut bytes = Vec::with_capacity(
        mixed_samples
            .len()
            .checked_mul(PCM_BYTES_PER_SAMPLE)
            .ok_or(PcmMixError::OutputTooLarge)?,
    );
    for sample in mixed_samples {
        let sample = clamp_i16(sample / source_count);
        bytes.extend_from_slice(&sample.to_le_bytes());
    }

    Ok(MixedPcmChunk {
        bytes,
        timestamp_100ns,
        duration_100ns,
    })
}

fn required_chunk(
    chunk: Option<PcmInputChunk<'_>>,
    kind: AudioInputKind,
    missing: PcmMixError,
) -> Result<PcmInputChunk<'_>, PcmMixError> {
    let chunk = chunk.ok_or(missing)?;
    if chunk.kind != kind {
        return Err(PcmMixError::WrongInputKind);
    }
    validate_chunk(chunk)?;
    Ok(chunk)
}

fn validate_chunk(chunk: PcmInputChunk<'_>) -> Result<(), PcmMixError> {
    validate_format(chunk.format).map_err(|_| PcmMixError::InvalidFormat)?;
    if chunk.timestamp_100ns < 0 || chunk.duration_100ns <= 0 {
        return Err(PcmMixError::InvalidInputTime);
    }
    let frame_bytes = pcm_frame_bytes(chunk.format)?;
    if chunk.bytes.is_empty() || !chunk.bytes.len().is_multiple_of(frame_bytes) {
        return Err(PcmMixError::InvalidInputBytes);
    }
    Ok(())
}

fn synced_output_shape(
    sources: &[PcmInputChunk<'_>],
    target_format: PcmFormat,
) -> Result<(i64, i64, usize), PcmMixError> {
    let first = sources[0];
    let output_frames = target_frame_count(first, target_format)?;
    for source in sources.iter().skip(1) {
        if source.timestamp_100ns != first.timestamp_100ns
            || source.duration_100ns != first.duration_100ns
            || target_frame_count(*source, target_format)? != output_frames
        {
            return Err(PcmMixError::UnsyncedInputs);
        }
    }
    Ok((first.timestamp_100ns, first.duration_100ns, output_frames))
}

fn accumulate_source(
    mixed_samples: &mut [i32],
    target_format: PcmFormat,
    source: PcmInputChunk<'_>,
    output_frames: usize,
) -> Result<(), PcmMixError> {
    let source_frame_count = source_frame_count(source)?;
    let target_channels = usize::from(target_format.channels);
    for target_frame in 0..output_frames {
        let source_frame = resampled_source_frame(
            target_frame,
            source_frame_count,
            source.format.sample_rate_hz,
            target_format.sample_rate_hz,
        )?;
        for target_channel in 0..target_channels {
            let mixed_index = target_frame
                .checked_mul(target_channels)
                .and_then(|index| index.checked_add(target_channel))
                .ok_or(PcmMixError::OutputTooLarge)?;
            mixed_samples[mixed_index] +=
                remapped_sample(source, source_frame, target_channel, target_channels)?;
        }
    }
    Ok(())
}

fn remapped_sample(
    source: PcmInputChunk<'_>,
    source_frame: usize,
    target_channel: usize,
    target_channels: usize,
) -> Result<i32, PcmMixError> {
    let source_channels = usize::from(source.format.channels);
    if source_channels == 1 {
        return Ok(i32::from(read_i16_sample(source, source_frame, 0)?));
    }
    if target_channels == 1 {
        let sum = (0..source_channels).try_fold(0_i32, |sum, channel| {
            read_i16_sample(source, source_frame, channel).map(|sample| sum + i32::from(sample))
        })?;
        let channels = i32::try_from(source_channels).map_err(|_| PcmMixError::OutputTooLarge)?;
        return Ok(sum / channels);
    }
    if target_channel < source_channels {
        return Ok(i32::from(read_i16_sample(
            source,
            source_frame,
            target_channel,
        )?));
    }
    let sum = (0..source_channels).try_fold(0_i32, |sum, channel| {
        read_i16_sample(source, source_frame, channel).map(|sample| sum + i32::from(sample))
    })?;
    let channels = i32::try_from(source_channels).map_err(|_| PcmMixError::OutputTooLarge)?;
    Ok(sum / channels)
}

fn read_i16_sample(
    source: PcmInputChunk<'_>,
    source_frame: usize,
    channel: usize,
) -> Result<i16, PcmMixError> {
    let source_channels = usize::from(source.format.channels);
    let sample_index = source_frame
        .checked_mul(source_channels)
        .and_then(|index| index.checked_add(channel))
        .ok_or(PcmMixError::OutputTooLarge)?;
    let byte_index = sample_index
        .checked_mul(PCM_BYTES_PER_SAMPLE)
        .ok_or(PcmMixError::OutputTooLarge)?;
    let bytes = source
        .bytes
        .get(byte_index..byte_index + PCM_BYTES_PER_SAMPLE)
        .ok_or(PcmMixError::InvalidInputBytes)?;
    Ok(i16::from_le_bytes([bytes[0], bytes[1]]))
}

fn resampled_source_frame(
    target_frame: usize,
    source_frame_count: usize,
    source_rate_hz: u32,
    target_rate_hz: u32,
) -> Result<usize, PcmMixError> {
    let source_frame = u128::try_from(target_frame)
        .map_err(|_| PcmMixError::OutputTooLarge)?
        .checked_mul(u128::from(source_rate_hz))
        .ok_or(PcmMixError::OutputTooLarge)?
        / u128::from(target_rate_hz);
    let source_frame = usize::try_from(source_frame).map_err(|_| PcmMixError::OutputTooLarge)?;
    Ok(source_frame.min(source_frame_count.saturating_sub(1)))
}

fn target_frame_count(
    source: PcmInputChunk<'_>,
    target_format: PcmFormat,
) -> Result<usize, PcmMixError> {
    let source_frames = source_frame_count(source)?;
    let target_frames = u128::try_from(source_frames)
        .map_err(|_| PcmMixError::OutputTooLarge)?
        .checked_mul(u128::from(target_format.sample_rate_hz))
        .ok_or(PcmMixError::OutputTooLarge)?
        .checked_add(u128::from(source.format.sample_rate_hz / 2))
        .ok_or(PcmMixError::OutputTooLarge)?
        / u128::from(source.format.sample_rate_hz);
    usize::try_from(target_frames)
        .map(|frames| frames.max(1))
        .map_err(|_| PcmMixError::OutputTooLarge)
}

fn source_frame_count(source: PcmInputChunk<'_>) -> Result<usize, PcmMixError> {
    Ok(source.bytes.len() / pcm_frame_bytes(source.format)?)
}

fn pcm_frame_bytes(format: PcmFormat) -> Result<usize, PcmMixError> {
    usize::from(format.channels)
        .checked_mul(PCM_BYTES_PER_SAMPLE)
        .ok_or(PcmMixError::OutputTooLarge)
}

fn clamp_i16(sample: i32) -> i16 {
    match i16::try_from(sample) {
        Ok(sample) => sample,
        Err(_) if sample < 0 => i16::MIN,
        Err(_) => i16::MAX,
    }
}
