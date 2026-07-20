//! Platform-neutral audio capability summary.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AudioUnavailableReason {
    #[cfg(test)]
    PermissionDenied,
    #[cfg(test)]
    PolicyDenied,
    #[cfg(test)]
    DeviceBusy,
    #[cfg(test)]
    AudioSourceLost,
    UnsupportedCapability,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MicrophoneDeviceDescriptor {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) is_default: bool,
    pub(crate) available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AudioCapabilitySummary {
    pub(crate) microphone_devices: Vec<MicrophoneDeviceDescriptor>,
    pub(crate) supports_microphone: bool,
    pub(crate) supports_system_audio: bool,
    pub(crate) supports_mixed_audio: bool,
    pub(crate) unavailable_reasons: Vec<AudioUnavailableReason>,
}

pub(crate) fn runtime_audio_capability_summary() -> AudioCapabilitySummary {
    platform_audio_capability_summary()
}

#[cfg(windows)]
fn platform_audio_capability_summary() -> AudioCapabilitySummary {
    super::devices::summarize_capabilities(
        &super::windows_devices::audio_endpoints().unwrap_or_default(),
    )
}

#[cfg(not(windows))]
fn platform_audio_capability_summary() -> AudioCapabilitySummary {
    AudioCapabilitySummary {
        microphone_devices: Vec::new(),
        supports_microphone: false,
        supports_system_audio: false,
        supports_mixed_audio: false,
        unavailable_reasons: vec![AudioUnavailableReason::UnsupportedCapability],
    }
}
