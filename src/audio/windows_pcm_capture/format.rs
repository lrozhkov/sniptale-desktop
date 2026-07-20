//! WASAPI PCM stream format and timing helpers.
#![cfg(windows)]

use windows::Win32::Media::Audio::{
    AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY, AUDCLNT_BUFFERFLAGS_SILENT,
    AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_LOOPBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, WAVE_FORMAT_PCM,
    WAVEFORMATEX,
};

use super::{
    super::mixer::{AudioInputKind, PcmFormat},
    WindowsWasapiPcmError,
};

const PCM_BITS_PER_SAMPLE: u16 = 16;
const PCM_BYTES_PER_SAMPLE: u16 = PCM_BITS_PER_SAMPLE / 8;
const HUNDRED_NS_PER_SECOND: u64 = 10_000_000;

pub(super) fn wave_format(format: PcmFormat) -> Result<WAVEFORMATEX, WindowsWasapiPcmError> {
    let block_align =
        u16::try_from(block_align(format)?).map_err(|_| WindowsWasapiPcmError::BufferTooLarge)?;
    let average_bytes_per_second = format
        .sample_rate_hz
        .checked_mul(u32::from(block_align))
        .ok_or(WindowsWasapiPcmError::BufferTooLarge)?;
    Ok(WAVEFORMATEX {
        wFormatTag: u16::try_from(WAVE_FORMAT_PCM)
            .map_err(|_| WindowsWasapiPcmError::InvalidFormat)?,
        nChannels: format.channels,
        nSamplesPerSec: format.sample_rate_hz,
        nAvgBytesPerSec: average_bytes_per_second,
        nBlockAlign: block_align,
        wBitsPerSample: PCM_BITS_PER_SAMPLE,
        cbSize: 0,
    })
}

pub(super) fn block_align(format: PcmFormat) -> Result<usize, WindowsWasapiPcmError> {
    if !(8_000..=192_000).contains(&format.sample_rate_hz) || !(1..=8).contains(&format.channels) {
        return Err(WindowsWasapiPcmError::InvalidFormat);
    }
    usize::from(format.channels)
        .checked_mul(usize::from(PCM_BYTES_PER_SAMPLE))
        .ok_or(WindowsWasapiPcmError::BufferTooLarge)
}

pub(super) fn stream_flags(kind: AudioInputKind) -> u32 {
    let flags = AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
    match kind {
        AudioInputKind::Microphone => flags,
        AudioInputKind::SystemLoopback => flags | AUDCLNT_STREAMFLAGS_LOOPBACK,
    }
}

pub(super) fn packet_duration_100ns(
    frames: u32,
    sample_rate_hz: u32,
) -> Result<i64, WindowsWasapiPcmError> {
    let duration = u64::from(frames)
        .checked_mul(HUNDRED_NS_PER_SECOND)
        .ok_or(WindowsWasapiPcmError::BufferTooLarge)?
        / u64::from(sample_rate_hz);
    i64::try_from(duration.max(1)).map_err(|_| WindowsWasapiPcmError::BufferTooLarge)
}

pub(super) const fn silent_flag() -> u32 {
    AUDCLNT_BUFFERFLAGS_SILENT.0 as u32
}

pub(super) const fn data_discontinuity_flag() -> u32 {
    AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY.0 as u32
}

pub(super) const fn timestamp_error_flag() -> u32 {
    AUDCLNT_BUFFERFLAGS_TIMESTAMP_ERROR.0 as u32
}
