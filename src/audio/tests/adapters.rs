use super::{available_system, target_format};
use crate::audio::{
    AudioUnavailableReason,
    devices::{AudioEndpoint, AudioEndpointState, AudioPlanError},
    loopback::system_loopback_input_spec,
    wasapi_capture::microphone_input_spec,
};

#[test]
fn source_adapters_reject_wrong_or_unavailable_endpoints() {
    let unavailable = AudioEndpoint::system_loopback(
        "system-default",
        "System Audio",
        AudioEndpointState::DeviceBusy,
    );
    assert!(matches!(
        microphone_input_spec(&unavailable, target_format(), 0),
        Err(AudioPlanError::WrongEndpointKind)
    ));
    assert!(matches!(
        system_loopback_input_spec(&unavailable, target_format(), 0),
        Err(AudioPlanError::SourceUnavailable(
            AudioUnavailableReason::DeviceBusy
        ))
    ));

    assert!(system_loopback_input_spec(&available_system(), target_format(), 0).is_ok());
}
