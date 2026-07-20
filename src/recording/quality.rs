//! Recording quality adapter for encoder reporting.

#[cfg(test)]
use super::encoder::{
    EncoderKind, EncoderPlan, H264Level, H264Profile, h264_level_for_plan, h264_profile_for_plan,
};
use crate::{
    protocol::recording::{NativeEffectiveQualitySettings, NativeRequestedQualitySettings},
    settings::effective::effective_recording_quality,
};

#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg(test)]
pub(crate) struct RecordingQualityReport {
    pub(crate) encoder: EncoderKind,
    pub(crate) h264_profile: H264Profile,
    pub(crate) h264_level: H264Level,
    pub(crate) video_bitrate_mbps: f64,
    pub(crate) audio_bitrate_kbps: u16,
    pub(crate) fps: u16,
}

pub(crate) use crate::settings::effective::{
    ALLOWED_AUDIO_BITRATE_KBPS, MAX_VIDEO_BITRATE_MBPS, MIN_VIDEO_BITRATE_MBPS,
    QualityLimits as RecordingQualityLimits, SettingsError as RecordingQualityError,
};

pub(crate) fn effective_quality(
    requested: &NativeRequestedQualitySettings,
    source_width: u32,
    source_height: u32,
    limits: RecordingQualityLimits,
) -> Result<NativeEffectiveQualitySettings, RecordingQualityError> {
    effective_recording_quality(requested, source_width, source_height, limits)
}

#[cfg(test)]
pub(crate) fn quality_report(
    plan: EncoderPlan,
) -> Result<RecordingQualityReport, super::encoder::EncoderPlanError> {
    Ok(RecordingQualityReport {
        encoder: plan.encoder,
        h264_profile: h264_profile_for_plan(plan),
        h264_level: h264_level_for_plan(plan)?,
        video_bitrate_mbps: plan.video_bitrate_mbps,
        audio_bitrate_kbps: plan.audio_bitrate_kbps,
        fps: plan.fps,
    })
}
