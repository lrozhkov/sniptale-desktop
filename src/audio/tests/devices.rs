use super::{available_microphone, available_system};
use crate::audio::{
    AudioUnavailableReason,
    devices::{
        AudioEndpoint, AudioEndpointState, AudioPlanError, AudioSourceMode, AudioSourceRequest,
        select_sources, summarize_capabilities,
    },
    runtime_audio_capability_summary,
};

#[test]
fn runtime_audio_capabilities_fail_closed_without_endpoint_probe() {
    let summary = runtime_audio_capability_summary();

    assert!(!summary.supports_microphone);
    assert!(!summary.supports_system_audio);
    assert!(!summary.supports_mixed_audio);
    assert_eq!(
        summary.unavailable_reasons,
        vec![AudioUnavailableReason::UnsupportedCapability]
    );
}

#[test]
fn capabilities_require_available_sources_for_mixed_audio() {
    let endpoints = [
        AudioEndpoint::microphone(
            "mic-default",
            "Default Mic",
            true,
            AudioEndpointState::PermissionDenied,
        ),
        AudioEndpoint::microphone(
            "policy-denied",
            "Policy Denied Mic",
            false,
            AudioEndpointState::PolicyDenied,
        ),
        available_system(),
    ];

    let summary = summarize_capabilities(&endpoints);

    assert!(!summary.supports_microphone);
    assert!(summary.supports_system_audio);
    assert!(!summary.supports_mixed_audio);
    assert_eq!(
        summary.unavailable_reasons,
        vec![
            AudioUnavailableReason::PermissionDenied,
            AudioUnavailableReason::PolicyDenied
        ]
    );
    assert!(!summary.microphone_devices[0].available);
}

#[test]
fn requested_microphone_id_does_not_fallback_to_another_device() {
    let endpoints = [
        AudioEndpoint::microphone(
            "requested",
            "Requested Mic",
            true,
            AudioEndpointState::Unplugged,
        ),
        available_microphone("alternate", false),
        available_system(),
    ];
    let request = AudioSourceRequest {
        mode: AudioSourceMode::Mixed,
        microphone_id: Some("requested"),
    };

    assert!(matches!(
        select_sources(request, &endpoints),
        Err(AudioPlanError::SourceUnavailable(
            AudioUnavailableReason::AudioSourceLost
        ))
    ));
}
