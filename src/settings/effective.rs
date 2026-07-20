//! Capability-clamped effective settings calculation.

#[cfg(any(windows, test))]
use serde_json::Number;

#[cfg(any(windows, test))]
use crate::protocol::recording::{
    NativeEffectiveEncoder, NativeEffectiveQualitySettings, NativeRequestedQualitySettings,
    NativeVideoFrameRate, NativeVideoQuality,
};
use crate::protocol::{
    NativeEffectiveSettings, NativeSettingsSnapshot, recording::NativeSettingsWarning,
};

pub(crate) const DEFERRED_EVALUATION_WARNING_CODE: &str = "deferred-evaluation";
#[cfg(any(windows, test))]
pub(crate) const MIN_VIDEO_BITRATE_MBPS: f64 = 2.0;
#[cfg(any(windows, test))]
pub(crate) const MAX_VIDEO_BITRATE_MBPS: f64 = 80.0;
#[cfg(any(windows, test))]
pub(crate) const ALLOWED_AUDIO_BITRATE_KBPS: [u16; 4] = [96, 128, 160, 192];

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SettingsError {
    InvalidSourceGeometry,
    SourceExceedsCapability,
    InvalidManualBitrate,
    InvalidNumber,
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct QualityLimits {
    pub(crate) max_fps: u16,
    pub(crate) max_width: u32,
    pub(crate) max_height: u32,
    pub(crate) hardware_encoder_available: bool,
}

#[must_use]
pub(crate) fn effective_settings_for_sync(
    settings: &NativeSettingsSnapshot,
) -> NativeEffectiveSettings {
    let warnings = sync_stage_warnings(settings);
    NativeEffectiveSettings {
        screenshots: settings.screenshots.clone(),
        video: settings.video.clone(),
        warnings,
    }
}

fn sync_stage_warnings(settings: &NativeSettingsSnapshot) -> Vec<NativeSettingsWarning> {
    if settings.video.enabled {
        vec![deferred_quality_warning()]
    } else {
        Vec::new()
    }
}

#[cfg(any(windows, test))]
pub(crate) fn effective_recording_quality(
    requested: &NativeRequestedQualitySettings,
    source_width: u32,
    source_height: u32,
    limits: QualityLimits,
) -> Result<NativeEffectiveQualitySettings, SettingsError> {
    validate_geometry(source_width, source_height, limits)?;

    let mut warnings = Vec::new();
    let effective_fps = effective_fps(requested, limits, &mut warnings);
    let effective_video_bitrate_mbps = effective_video_bitrate(
        requested,
        source_width,
        source_height,
        effective_fps,
        &mut warnings,
    )?;
    let encoder = effective_encoder(limits, &mut warnings);

    Ok(NativeEffectiveQualitySettings {
        audio_bitrate_kbps: requested.audio_bitrate_kbps,
        audio_source_mode: requested.audio_source_mode.clone(),
        frame_rate: requested.frame_rate.clone(),
        quality: requested.quality.clone(),
        video_bitrate_mbps_override: requested.video_bitrate_mbps_override.clone(),
        effective_audio_bitrate_kbps: Number::from(requested.audio_bitrate_kbps),
        effective_fps: Number::from(effective_fps),
        effective_video_bitrate_mbps: Number::from_f64(effective_video_bitrate_mbps)
            .ok_or(SettingsError::InvalidNumber)?,
        encoder,
        warnings,
    })
}

#[cfg(any(windows, test))]
fn validate_geometry(
    source_width: u32,
    source_height: u32,
    limits: QualityLimits,
) -> Result<(), SettingsError> {
    if source_width == 0 || source_height == 0 || limits.max_fps == 0 {
        return Err(SettingsError::InvalidSourceGeometry);
    }
    if source_width > limits.max_width || source_height > limits.max_height {
        return Err(SettingsError::SourceExceedsCapability);
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn effective_fps(
    requested: &NativeRequestedQualitySettings,
    limits: QualityLimits,
    warnings: &mut Vec<NativeSettingsWarning>,
) -> u16 {
    let requested_fps = requested
        .frame_rate
        .fps()
        .unwrap_or(requested.quality.fps_cap());
    let preset_capped_fps = requested_fps.min(requested.quality.fps_cap());
    let effective_fps = preset_capped_fps.min(limits.max_fps);
    if effective_fps != requested_fps {
        warnings.push(settings_warning("clamped", Some("frameRate"), None));
    }
    effective_fps
}

#[cfg(any(windows, test))]
fn effective_video_bitrate(
    requested: &NativeRequestedQualitySettings,
    source_width: u32,
    source_height: u32,
    effective_fps: u16,
    warnings: &mut Vec<NativeSettingsWarning>,
) -> Result<f64, SettingsError> {
    if let Some(override_mbps) = requested
        .video_bitrate_mbps_override
        .as_ref()
        .and_then(Number::as_f64)
    {
        if !override_mbps.is_finite() {
            return Err(SettingsError::InvalidManualBitrate);
        }
        let bounded = override_mbps.clamp(MIN_VIDEO_BITRATE_MBPS, MAX_VIDEO_BITRATE_MBPS);
        if (bounded - override_mbps).abs() > f64::EPSILON {
            warnings.push(settings_warning(
                "clamped",
                Some("videoBitrateMbpsOverride"),
                None,
            ));
        }
        return Ok(bounded);
    }

    let preset = requested.quality.bitrate_preset();
    let target = f64::from(source_width)
        * f64::from(source_height)
        * f64::from(effective_fps)
        * preset.bits_per_pixel_frame
        / 1_000_000.0;
    Ok(target.clamp(preset.min_mbps, preset.max_mbps))
}

#[cfg(any(windows, test))]
fn effective_encoder(
    limits: QualityLimits,
    warnings: &mut Vec<NativeSettingsWarning>,
) -> NativeEffectiveEncoder {
    if limits.hardware_encoder_available {
        NativeEffectiveEncoder::Hardware
    } else {
        warnings.push(settings_warning(
            "unsupported-capability",
            Some("encoder"),
            None,
        ));
        NativeEffectiveEncoder::Software
    }
}

fn deferred_quality_warning() -> NativeSettingsWarning {
    settings_warning(
        DEFERRED_EVALUATION_WARNING_CODE,
        Some("video.effectiveQuality"),
        Some("recording quality evaluation is deferred until source geometry is known"),
    )
}

fn settings_warning(
    code: &str,
    field: Option<&str>,
    message: Option<&str>,
) -> NativeSettingsWarning {
    NativeSettingsWarning {
        code: code.to_owned(),
        field: field.map(str::to_owned),
        message: message.map(str::to_owned),
    }
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug)]
struct BitratePreset {
    bits_per_pixel_frame: f64,
    min_mbps: f64,
    max_mbps: f64,
}

#[cfg(any(windows, test))]
impl NativeVideoQuality {
    const fn fps_cap(&self) -> u16 {
        match self {
            Self::Low => 24,
            Self::Medium | Self::High => 30,
            Self::Ultra => 60,
        }
    }

    const fn bitrate_preset(&self) -> BitratePreset {
        match self {
            Self::Low => BitratePreset {
                bits_per_pixel_frame: 0.055,
                min_mbps: MIN_VIDEO_BITRATE_MBPS,
                max_mbps: 25.0,
            },
            Self::Medium => BitratePreset {
                bits_per_pixel_frame: 0.085,
                min_mbps: 4.0,
                max_mbps: 45.0,
            },
            Self::High => BitratePreset {
                bits_per_pixel_frame: 0.130,
                min_mbps: 6.0,
                max_mbps: 70.0,
            },
            Self::Ultra => BitratePreset {
                bits_per_pixel_frame: 0.180,
                min_mbps: 8.0,
                max_mbps: MAX_VIDEO_BITRATE_MBPS,
            },
        }
    }
}

#[cfg(any(windows, test))]
impl NativeVideoFrameRate {
    const fn fps(&self) -> Option<u16> {
        match self {
            Self::Auto => None,
            Self::Fps24 => Some(24),
            Self::Fps30 => Some(30),
            Self::Fps60 => Some(60),
        }
    }
}
