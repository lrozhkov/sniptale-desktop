//! WASAPI microphone capture owner.
use super::devices::{AudioEndpoint, AudioEndpointKind, AudioPlanError, available_endpoint};
use super::mixer::{AudioInputKind, AudioInputSpec, PcmFormat};

pub(crate) fn microphone_input_spec(
    endpoint: &AudioEndpoint,
    format: PcmFormat,
    clock_drift_ppm: i32,
) -> Result<AudioInputSpec, AudioPlanError> {
    if endpoint.kind != AudioEndpointKind::Microphone {
        return Err(AudioPlanError::WrongEndpointKind);
    }
    available_endpoint(endpoint, AudioPlanError::MissingMicrophone).map(|_| AudioInputSpec {
        kind: AudioInputKind::Microphone,
        format,
        clock_drift_ppm,
    })
}
