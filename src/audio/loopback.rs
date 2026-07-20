//! WASAPI system audio loopback owner.
use super::devices::{AudioEndpoint, AudioEndpointKind, AudioPlanError, available_endpoint};
use super::mixer::{AudioInputKind, AudioInputSpec, PcmFormat};

pub(crate) fn system_loopback_input_spec(
    endpoint: &AudioEndpoint,
    format: PcmFormat,
    clock_drift_ppm: i32,
) -> Result<AudioInputSpec, AudioPlanError> {
    if endpoint.kind != AudioEndpointKind::SystemLoopback {
        return Err(AudioPlanError::WrongEndpointKind);
    }
    available_endpoint(endpoint, AudioPlanError::MissingSystemAudio).map(|_| AudioInputSpec {
        kind: AudioInputKind::SystemLoopback,
        format,
        clock_drift_ppm,
    })
}
