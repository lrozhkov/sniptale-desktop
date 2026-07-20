//! Windows recording audio pipeline composition.
#![cfg(windows)]

use crate::audio::{
    AudioSourceSelection, MixPlan, PcmMixPipeline, WindowsWasapiPcmError, WindowsWasapiPcmSource,
};

use super::RecordingRuntimeError;

pub(super) type WindowsAudioPipeline =
    PcmMixPipeline<WindowsWasapiPcmSource, WindowsWasapiPcmSource>;

pub(super) fn windows_audio_pipeline(
    selection: &AudioSourceSelection,
    mix: MixPlan,
) -> Result<WindowsAudioPipeline, RecordingRuntimeError> {
    let microphone = selection
        .microphone
        .as_ref()
        .map(|endpoint| WindowsWasapiPcmSource::open_microphone(endpoint, mix.target_format))
        .transpose()
        .map_err(wasapi_error)?;
    let system_loopback = selection
        .system_loopback
        .as_ref()
        .map(|endpoint| WindowsWasapiPcmSource::open_system_loopback(endpoint, mix.target_format))
        .transpose()
        .map_err(wasapi_error)?;
    Ok(PcmMixPipeline::new(mix, microphone, system_loopback))
}

fn wasapi_error(error: WindowsWasapiPcmError) -> RecordingRuntimeError {
    tracing::warn!(
        target: "sniptale_native_agent::recording",
        phase = "audio-source",
        code = ?error,
        "recording runtime start failed"
    );
    RecordingRuntimeError::RuntimeUnavailable
}
