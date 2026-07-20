//! Platform-neutral recording runtime planning.
use crate::{
    audio::{
        AudioEndpoint, AudioInputSpec, AudioPlanError, AudioSourceMode, AudioSourceRequest,
        AudioSourceSelection, MixPlan, MixPlanError, PcmFormat, microphone_input_spec, plan_mix,
        select_sources, system_loopback_input_spec,
    },
    protocol::NativeEffectiveQualitySettings,
    protocol::recording::{NativeAudioSourceMode, NativeEffectiveEncoder},
    recording::RecordingStartRequest,
};

use super::super::{
    encoder::{
        EncoderAudioMode, EncoderCapabilities, EncoderPlan, EncoderPlanError, EncoderRequest,
        plan_encoder,
    },
    quality::{RecordingQualityError, RecordingQualityLimits, effective_quality},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RecordingSourceGeometry {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RecordingAudioFacts {
    pub(crate) target_format: PcmFormat,
    pub(crate) microphone_format: PcmFormat,
    pub(crate) system_loopback_format: PcmFormat,
    pub(crate) microphone_clock_drift_ppm: i32,
    pub(crate) system_loopback_clock_drift_ppm: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RecordingRuntimePlan {
    pub(crate) source: RecordingSourceGeometry,
    pub(crate) audio_selection: AudioSourceSelection,
    pub(crate) mix: MixPlan,
    pub(crate) encoder: EncoderPlan,
    pub(crate) effective_quality: NativeEffectiveQualitySettings,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecordingRuntimePlanError {
    Audio(AudioPlanError),
    Mix(MixPlanError),
    Encoder(EncoderPlanError),
    Quality(RecordingQualityError),
    InvalidEffectiveQuality,
}

pub(crate) fn plan_recording_runtime(
    request: &RecordingStartRequest,
    source: RecordingSourceGeometry,
    audio_endpoints: &[AudioEndpoint],
    audio_facts: RecordingAudioFacts,
    encoder_capabilities: EncoderCapabilities,
    quality_limits: RecordingQualityLimits,
) -> Result<RecordingRuntimePlan, RecordingRuntimePlanError> {
    let effective_quality = effective_quality(
        &request.requested_quality,
        source.width,
        source.height,
        quality_limits,
    )
    .map_err(RecordingRuntimePlanError::Quality)?;
    let audio_mode = audio_source_mode(&request.requested_quality.audio_source_mode);
    let audio_selection = select_sources(
        AudioSourceRequest {
            mode: audio_mode,
            microphone_id: request.source.microphone_device_id.as_deref(),
        },
        audio_endpoints,
    )
    .map_err(RecordingRuntimePlanError::Audio)?;
    let microphone = audio_input_spec(
        audio_selection.microphone.as_ref(),
        audio_facts.microphone_format,
        audio_facts.microphone_clock_drift_ppm,
        microphone_input_spec,
    )?;
    let system_loopback = audio_input_spec(
        audio_selection.system_loopback.as_ref(),
        audio_facts.system_loopback_format,
        audio_facts.system_loopback_clock_drift_ppm,
        system_loopback_input_spec,
    )?;
    let mix = plan_mix(
        audio_mode,
        microphone,
        system_loopback,
        audio_facts.target_format,
    )
    .map_err(RecordingRuntimePlanError::Mix)?;
    let encoder = plan_encoder(
        EncoderRequest {
            width: source.width,
            height: source.height,
            fps: effective_u16(&effective_quality.effective_fps)?,
            video_bitrate_mbps: effective_f64(&effective_quality.effective_video_bitrate_mbps)?,
            audio_bitrate_kbps: effective_u16(&effective_quality.effective_audio_bitrate_kbps)?,
            audio_mode: encoder_audio_mode(audio_mode),
            prefer_hardware: matches!(effective_quality.encoder, NativeEffectiveEncoder::Hardware),
        },
        encoder_capabilities,
    )
    .map_err(RecordingRuntimePlanError::Encoder)?;

    Ok(RecordingRuntimePlan {
        source,
        audio_selection,
        mix,
        encoder,
        effective_quality,
    })
}

fn audio_input_spec<F>(
    endpoint: Option<&AudioEndpoint>,
    format: PcmFormat,
    clock_drift_ppm: i32,
    spec: F,
) -> Result<Option<AudioInputSpec>, RecordingRuntimePlanError>
where
    F: Fn(&AudioEndpoint, PcmFormat, i32) -> Result<AudioInputSpec, AudioPlanError>,
{
    endpoint
        .map(|endpoint| spec(endpoint, format, clock_drift_ppm))
        .transpose()
        .map_err(RecordingRuntimePlanError::Audio)
}

const fn audio_source_mode(mode: &NativeAudioSourceMode) -> AudioSourceMode {
    match mode {
        NativeAudioSourceMode::Microphone => AudioSourceMode::Microphone,
        NativeAudioSourceMode::System => AudioSourceMode::System,
        NativeAudioSourceMode::Mixed => AudioSourceMode::Mixed,
    }
}

const fn encoder_audio_mode(mode: AudioSourceMode) -> EncoderAudioMode {
    match mode {
        AudioSourceMode::Microphone => EncoderAudioMode::Microphone,
        AudioSourceMode::System => EncoderAudioMode::System,
        AudioSourceMode::Mixed => EncoderAudioMode::Mixed,
    }
}

fn effective_u16(value: &serde_json::Number) -> Result<u16, RecordingRuntimePlanError> {
    value
        .as_u64()
        .and_then(|value| u16::try_from(value).ok())
        .ok_or(RecordingRuntimePlanError::InvalidEffectiveQuality)
}

fn effective_f64(value: &serde_json::Number) -> Result<f64, RecordingRuntimePlanError> {
    value
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or(RecordingRuntimePlanError::InvalidEffectiveQuality)
}
