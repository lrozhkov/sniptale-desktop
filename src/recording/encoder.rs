//! Media Foundation Sink Writer owner.
#[cfg(any(windows, test))]
mod runtime_owner;
pub(crate) mod samples;
#[cfg(windows)]
pub(crate) mod windows_sink_writer;

use crate::audio::MixPlan;

use super::quality::{ALLOWED_AUDIO_BITRATE_KBPS, MAX_VIDEO_BITRATE_MBPS, MIN_VIDEO_BITRATE_MBPS};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EncoderKind {
    Hardware,
    Software,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum H264Profile {
    Main,
    High,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum H264Level {
    L4_0,
    L4_1,
    L4_2,
    L5_0,
    L5_1,
    L5_2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EncoderAudioMode {
    Microphone,
    System,
    Mixed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EncoderPlanError {
    InvalidGeometry,
    InvalidFrameRate,
    InvalidVideoBitrate,
    InvalidAudioBitrate,
    EncoderUnavailable,
    UnsupportedH264Level,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EncoderRequest {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) fps: u16,
    pub(crate) video_bitrate_mbps: f64,
    pub(crate) audio_bitrate_kbps: u16,
    pub(crate) audio_mode: EncoderAudioMode,
    pub(crate) prefer_hardware: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct EncoderCapabilities {
    pub(crate) max_width: u32,
    pub(crate) max_height: u32,
    pub(crate) max_fps: u16,
    pub(crate) hardware_available: bool,
    pub(crate) software_available: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EncoderPlan {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) fps: u16,
    pub(crate) video_bitrate_mbps: f64,
    pub(crate) audio_bitrate_kbps: u16,
    pub(crate) audio_mode: EncoderAudioMode,
    pub(crate) encoder: EncoderKind,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Mp4SinkWriterPlan {
    pub(crate) video: H264SinkWriterStreamPlan,
    pub(crate) audio: AacSinkWriterStreamPlan,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct H264SinkWriterStreamPlan {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) fps_numerator: u32,
    pub(crate) fps_denominator: u32,
    pub(crate) frame_duration_100ns: u64,
    pub(crate) average_bitrate_bps: u32,
    pub(crate) top_down_stride_bytes: u32,
    pub(crate) profile: H264Profile,
    pub(crate) level: H264Level,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AacSinkWriterStreamPlan {
    pub(crate) sample_rate_hz: u32,
    pub(crate) channels: u16,
    pub(crate) average_bitrate_bps: u32,
    pub(crate) average_bytes_per_second: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SinkWriterPlanError {
    FrameRate,
    InvalidVideoGeometry,
    VideoBitrate,
    AudioBitrate,
    AudioFormat,
    UnsupportedH264Level,
}

pub(crate) fn plan_encoder(
    request: EncoderRequest,
    capabilities: EncoderCapabilities,
) -> Result<EncoderPlan, EncoderPlanError> {
    validate_geometry(request, capabilities)?;
    validate_frame_rate(request, capabilities)?;
    validate_bitrates(request)?;
    let encoder = select_encoder(request.prefer_hardware, capabilities)?;

    let plan = EncoderPlan {
        width: request.width,
        height: request.height,
        fps: request.fps,
        video_bitrate_mbps: request.video_bitrate_mbps,
        audio_bitrate_kbps: request.audio_bitrate_kbps,
        audio_mode: request.audio_mode,
        encoder,
    };
    h264_level_for_plan(plan)?;
    Ok(plan)
}

pub(crate) fn build_mp4_sink_writer_plan(
    encoder: EncoderPlan,
    mix: &MixPlan,
) -> Result<Mp4SinkWriterPlan, SinkWriterPlanError> {
    Ok(Mp4SinkWriterPlan {
        video: h264_stream_plan(encoder)?,
        audio: aac_stream_plan(encoder, mix)?,
    })
}

pub(crate) fn h264_profile_for_plan(plan: EncoderPlan) -> H264Profile {
    if plan.width >= 2560 || plan.height >= 1440 || plan.video_bitrate_mbps >= 20.0 {
        H264Profile::High
    } else {
        H264Profile::Main
    }
}

pub(crate) fn h264_level_for_plan(plan: EncoderPlan) -> Result<H264Level, EncoderPlanError> {
    let macroblock_width = u64::from(plan.width).div_ceil(16);
    let macroblock_height = u64::from(plan.height).div_ceil(16);
    let macroblocks_per_frame = macroblock_width.saturating_mul(macroblock_height);
    let macroblocks_per_second = macroblocks_per_frame.saturating_mul(u64::from(plan.fps));
    if macroblocks_per_frame <= 8_192 && macroblocks_per_second <= 245_760 {
        if plan.video_bitrate_mbps > 20.0 {
            Ok(H264Level::L4_1)
        } else {
            Ok(H264Level::L4_0)
        }
    } else if macroblocks_per_frame <= 8_704 && macroblocks_per_second <= 522_240 {
        Ok(H264Level::L4_2)
    } else if macroblocks_per_frame <= 22_080 && macroblocks_per_second <= 589_824 {
        Ok(H264Level::L5_0)
    } else if macroblocks_per_frame <= 36_864 && macroblocks_per_second <= 983_040 {
        Ok(H264Level::L5_1)
    } else if macroblocks_per_frame <= 36_864 && macroblocks_per_second <= 2_073_600 {
        Ok(H264Level::L5_2)
    } else {
        Err(EncoderPlanError::UnsupportedH264Level)
    }
}

fn h264_stream_plan(encoder: EncoderPlan) -> Result<H264SinkWriterStreamPlan, SinkWriterPlanError> {
    if encoder.fps == 0 {
        return Err(SinkWriterPlanError::FrameRate);
    }
    let average_bitrate_bps =
        mbps_to_bps(encoder.video_bitrate_mbps).ok_or(SinkWriterPlanError::VideoBitrate)?;
    let top_down_stride_bytes = encoder
        .width
        .checked_mul(4)
        .filter(|stride| i32::try_from(*stride).is_ok())
        .ok_or(SinkWriterPlanError::InvalidVideoGeometry)?;
    Ok(H264SinkWriterStreamPlan {
        width: encoder.width,
        height: encoder.height,
        fps_numerator: u32::from(encoder.fps),
        fps_denominator: 1,
        frame_duration_100ns: 10_000_000_u64 / u64::from(encoder.fps),
        average_bitrate_bps,
        top_down_stride_bytes,
        profile: h264_profile_for_plan(encoder),
        level: h264_level_for_plan(encoder)
            .map_err(|_| SinkWriterPlanError::UnsupportedH264Level)?,
    })
}

fn aac_stream_plan(
    encoder: EncoderPlan,
    mix: &MixPlan,
) -> Result<AacSinkWriterStreamPlan, SinkWriterPlanError> {
    validate_aac_format(mix)?;
    let average_bitrate_bps = u32::from(encoder.audio_bitrate_kbps)
        .checked_mul(1_000)
        .ok_or(SinkWriterPlanError::AudioBitrate)?;
    Ok(AacSinkWriterStreamPlan {
        sample_rate_hz: mix.target_format.sample_rate_hz,
        channels: mix.target_format.channels,
        average_bitrate_bps,
        average_bytes_per_second: average_bitrate_bps / 8,
    })
}

fn validate_aac_format(mix: &MixPlan) -> Result<(), SinkWriterPlanError> {
    if !matches!(mix.target_format.sample_rate_hz, 44_100 | 48_000) {
        return Err(SinkWriterPlanError::AudioFormat);
    }
    if !matches!(mix.target_format.channels, 1 | 2) {
        return Err(SinkWriterPlanError::AudioFormat);
    }
    Ok(())
}

fn mbps_to_bps(value: f64) -> Option<u32> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    let rounded_bps = (value * 1_000_000.0).round();
    if rounded_bps > f64::from(u32::MAX) {
        return None;
    }
    format!("{rounded_bps:.0}").parse::<u32>().ok()
}

fn validate_geometry(
    request: EncoderRequest,
    capabilities: EncoderCapabilities,
) -> Result<(), EncoderPlanError> {
    if request.width == 0 || request.height == 0 {
        return Err(EncoderPlanError::InvalidGeometry);
    }
    if request.width > capabilities.max_width || request.height > capabilities.max_height {
        return Err(EncoderPlanError::InvalidGeometry);
    }
    Ok(())
}

fn validate_frame_rate(
    request: EncoderRequest,
    capabilities: EncoderCapabilities,
) -> Result<(), EncoderPlanError> {
    if request.fps == 0 || request.fps > capabilities.max_fps {
        return Err(EncoderPlanError::InvalidFrameRate);
    }
    Ok(())
}

fn validate_bitrates(request: EncoderRequest) -> Result<(), EncoderPlanError> {
    if !request.video_bitrate_mbps.is_finite()
        || !(MIN_VIDEO_BITRATE_MBPS..=MAX_VIDEO_BITRATE_MBPS).contains(&request.video_bitrate_mbps)
    {
        return Err(EncoderPlanError::InvalidVideoBitrate);
    }
    if !ALLOWED_AUDIO_BITRATE_KBPS.contains(&request.audio_bitrate_kbps) {
        return Err(EncoderPlanError::InvalidAudioBitrate);
    }
    Ok(())
}

fn select_encoder(
    prefer_hardware: bool,
    capabilities: EncoderCapabilities,
) -> Result<EncoderKind, EncoderPlanError> {
    if prefer_hardware && capabilities.hardware_available {
        return Ok(EncoderKind::Hardware);
    }
    if capabilities.software_available {
        return Ok(EncoderKind::Software);
    }
    if capabilities.hardware_available {
        return Ok(EncoderKind::Hardware);
    }
    Err(EncoderPlanError::EncoderUnavailable)
}
