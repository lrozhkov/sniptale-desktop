//! Media Foundation media-type construction for the MP4 Sink Writer.
#![cfg(windows)]
#![allow(unsafe_code)]

use windows::Win32::Media::MediaFoundation::{
    IMFAttributes, IMFMediaType, IMFSinkWriter, MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION,
    MF_MT_AAC_PAYLOAD_TYPE, MF_MT_AUDIO_AVG_BYTES_PER_SECOND, MF_MT_AUDIO_BITS_PER_SAMPLE,
    MF_MT_AUDIO_BLOCK_ALIGNMENT, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND,
    MF_MT_AVG_BITRATE, MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
    MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE, MF_MT_MPEG2_LEVEL, MF_MT_MPEG2_PROFILE,
    MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MFAudioFormat_AAC, MFAudioFormat_PCM,
    MFCreateMediaType, MFMediaType_Audio, MFMediaType_Video, MFVideoFormat_H264,
    MFVideoFormat_RGB32, MFVideoInterlace_Progressive, eAVEncH264VLevel4, eAVEncH264VLevel4_1,
    eAVEncH264VLevel4_2, eAVEncH264VLevel5, eAVEncH264VLevel5_1, eAVEncH264VLevel5_2,
    eAVEncH264VProfile_High, eAVEncH264VProfile_Main,
};

use super::{Mp4SinkWriterError, to_error};
use crate::recording::encoder::{
    AacSinkWriterStreamPlan, H264Level, H264Profile, H264SinkWriterStreamPlan,
    samples::{PCM_BITS_PER_SAMPLE, PCM_BYTES_PER_SAMPLE},
};

const AAC_LC_LEVEL_2: u32 = 0x29;

pub(super) fn add_h264_stream(
    writer: &IMFSinkWriter,
    plan: &H264SinkWriterStreamPlan,
) -> Result<u32, Mp4SinkWriterError> {
    let media_type = h264_output_type(plan)?;
    // SAFETY: media_type is a complete H.264 output type owned for the duration of this call.
    unsafe { writer.AddStream(&media_type) }.map_err(to_error)
}

pub(super) fn add_aac_stream(
    writer: &IMFSinkWriter,
    plan: &AacSinkWriterStreamPlan,
) -> Result<u32, Mp4SinkWriterError> {
    let media_type = aac_output_type(plan)?;
    // SAFETY: media_type is a complete AAC output type owned for the duration of this call.
    unsafe { writer.AddStream(&media_type) }.map_err(to_error)
}

pub(super) fn set_rgb32_input_type(
    writer: &IMFSinkWriter,
    stream_index: u32,
    plan: &H264SinkWriterStreamPlan,
) -> Result<(), Mp4SinkWriterError> {
    let media_type = rgb32_input_type(plan)?;
    // SAFETY: stream_index came from AddStream on this writer and media_type describes validated
    // RGB32 frame input for the configured H.264 output stream.
    unsafe { writer.SetInputMediaType(stream_index, &media_type, None::<&IMFAttributes>) }
        .map_err(to_error)
}

pub(super) fn set_pcm_input_type(
    writer: &IMFSinkWriter,
    stream_index: u32,
    plan: &AacSinkWriterStreamPlan,
) -> Result<(), Mp4SinkWriterError> {
    let media_type = pcm_input_type(plan)?;
    // SAFETY: stream_index came from AddStream on this writer and media_type describes validated
    // PCM input for the configured AAC output stream.
    unsafe { writer.SetInputMediaType(stream_index, &media_type, None::<&IMFAttributes>) }
        .map_err(to_error)
}

fn h264_output_type(plan: &H264SinkWriterStreamPlan) -> Result<IMFMediaType, Mp4SinkWriterError> {
    let media_type = media_type()?;
    set_guid(&media_type, &MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
    set_guid(&media_type, &MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
    set_u32(&media_type, &MF_MT_AVG_BITRATE, plan.average_bitrate_bps)?;
    set_u32(&media_type, &MF_MT_INTERLACE_MODE, interlace_progressive()?)?;
    set_ratio(&media_type, &MF_MT_FRAME_SIZE, plan.width, plan.height)?;
    set_ratio(
        &media_type,
        &MF_MT_FRAME_RATE,
        plan.fps_numerator,
        plan.fps_denominator,
    )?;
    set_ratio(&media_type, &MF_MT_PIXEL_ASPECT_RATIO, 1, 1)?;
    set_u32(
        &media_type,
        &MF_MT_MPEG2_PROFILE,
        h264_profile(plan.profile)?,
    )?;
    set_u32(&media_type, &MF_MT_MPEG2_LEVEL, h264_level(plan.level)?)?;
    Ok(media_type)
}

fn rgb32_input_type(plan: &H264SinkWriterStreamPlan) -> Result<IMFMediaType, Mp4SinkWriterError> {
    let media_type = media_type()?;
    set_guid(&media_type, &MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
    set_guid(&media_type, &MF_MT_SUBTYPE, &MFVideoFormat_RGB32)?;
    set_u32(&media_type, &MF_MT_INTERLACE_MODE, interlace_progressive()?)?;
    set_ratio(&media_type, &MF_MT_FRAME_SIZE, plan.width, plan.height)?;
    set_ratio(
        &media_type,
        &MF_MT_FRAME_RATE,
        plan.fps_numerator,
        plan.fps_denominator,
    )?;
    set_ratio(&media_type, &MF_MT_PIXEL_ASPECT_RATIO, 1, 1)?;
    set_u32(
        &media_type,
        &MF_MT_DEFAULT_STRIDE,
        plan.top_down_stride_bytes,
    )?;
    Ok(media_type)
}

fn aac_output_type(plan: &AacSinkWriterStreamPlan) -> Result<IMFMediaType, Mp4SinkWriterError> {
    let media_type = media_type()?;
    set_guid(&media_type, &MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
    set_guid(&media_type, &MF_MT_SUBTYPE, &MFAudioFormat_AAC)?;
    set_u32(
        &media_type,
        &MF_MT_AUDIO_NUM_CHANNELS,
        u32::from(plan.channels),
    )?;
    set_u32(
        &media_type,
        &MF_MT_AUDIO_SAMPLES_PER_SECOND,
        plan.sample_rate_hz,
    )?;
    set_u32(
        &media_type,
        &MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
        plan.average_bytes_per_second,
    )?;
    set_u32(
        &media_type,
        &MF_MT_AUDIO_BITS_PER_SAMPLE,
        PCM_BITS_PER_SAMPLE,
    )?;
    set_u32(&media_type, &MF_MT_AAC_PAYLOAD_TYPE, 0)?;
    set_u32(
        &media_type,
        &MF_MT_AAC_AUDIO_PROFILE_LEVEL_INDICATION,
        AAC_LC_LEVEL_2,
    )?;
    set_u32(&media_type, &MF_MT_AVG_BITRATE, plan.average_bitrate_bps)?;
    Ok(media_type)
}

fn pcm_input_type(plan: &AacSinkWriterStreamPlan) -> Result<IMFMediaType, Mp4SinkWriterError> {
    let block_alignment = u32::from(plan.channels)
        .checked_mul(PCM_BYTES_PER_SAMPLE)
        .ok_or(Mp4SinkWriterError::InvalidCodecValue)?;
    let average_bytes_per_second = plan
        .sample_rate_hz
        .checked_mul(block_alignment)
        .ok_or(Mp4SinkWriterError::InvalidCodecValue)?;
    let media_type = media_type()?;
    set_guid(&media_type, &MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
    set_guid(&media_type, &MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
    set_u32(
        &media_type,
        &MF_MT_AUDIO_NUM_CHANNELS,
        u32::from(plan.channels),
    )?;
    set_u32(
        &media_type,
        &MF_MT_AUDIO_SAMPLES_PER_SECOND,
        plan.sample_rate_hz,
    )?;
    set_u32(
        &media_type,
        &MF_MT_AUDIO_BITS_PER_SAMPLE,
        PCM_BITS_PER_SAMPLE,
    )?;
    set_u32(&media_type, &MF_MT_AUDIO_BLOCK_ALIGNMENT, block_alignment)?;
    set_u32(
        &media_type,
        &MF_MT_AUDIO_AVG_BYTES_PER_SECOND,
        average_bytes_per_second,
    )?;
    Ok(media_type)
}

fn media_type() -> Result<IMFMediaType, Mp4SinkWriterError> {
    // SAFETY: creates a new empty Media Foundation media type owned by COM RAII.
    unsafe { MFCreateMediaType() }.map_err(to_error)
}

fn set_guid(
    media_type: &IMFMediaType,
    key: &windows::core::GUID,
    value: &windows::core::GUID,
) -> Result<(), Mp4SinkWriterError> {
    // SAFETY: key/value are stable GUID references and media_type is a live IMFMediaType.
    unsafe { media_type.SetGUID(key, value) }.map_err(to_error)
}

fn set_u32(
    media_type: &IMFMediaType,
    key: &windows::core::GUID,
    value: u32,
) -> Result<(), Mp4SinkWriterError> {
    // SAFETY: key is a stable GUID reference and media_type is a live IMFMediaType.
    unsafe { media_type.SetUINT32(key, value) }.map_err(to_error)
}

fn set_ratio(
    media_type: &IMFMediaType,
    key: &windows::core::GUID,
    numerator: u32,
    denominator: u32,
) -> Result<(), Mp4SinkWriterError> {
    if numerator == 0 || denominator == 0 {
        return Err(Mp4SinkWriterError::InvalidCodecValue);
    }
    let packed = (u64::from(numerator) << 32) | u64::from(denominator);
    // SAFETY: key is a stable GUID reference and media_type is a live IMFMediaType.
    unsafe { media_type.SetUINT64(key, packed) }.map_err(to_error)
}

fn h264_profile(profile: H264Profile) -> Result<u32, Mp4SinkWriterError> {
    match profile {
        H264Profile::Main => u32::try_from(eAVEncH264VProfile_Main.0),
        H264Profile::High => u32::try_from(eAVEncH264VProfile_High.0),
    }
    .map_err(|_| Mp4SinkWriterError::InvalidCodecValue)
}

fn h264_level(level: H264Level) -> Result<u32, Mp4SinkWriterError> {
    match level {
        H264Level::L4_0 => u32::try_from(eAVEncH264VLevel4.0),
        H264Level::L4_1 => u32::try_from(eAVEncH264VLevel4_1.0),
        H264Level::L4_2 => u32::try_from(eAVEncH264VLevel4_2.0),
        H264Level::L5_0 => u32::try_from(eAVEncH264VLevel5.0),
        H264Level::L5_1 => u32::try_from(eAVEncH264VLevel5_1.0),
        H264Level::L5_2 => u32::try_from(eAVEncH264VLevel5_2.0),
    }
    .map_err(|_| Mp4SinkWriterError::InvalidCodecValue)
}

fn interlace_progressive() -> Result<u32, Mp4SinkWriterError> {
    u32::try_from(MFVideoInterlace_Progressive.0).map_err(|_| Mp4SinkWriterError::InvalidCodecValue)
}
