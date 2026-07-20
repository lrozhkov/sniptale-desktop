//! Audio endpoint discovery and capability owner.
#[cfg(any(windows, test))]
use super::capabilities::{
    AudioCapabilitySummary, AudioUnavailableReason, MicrophoneDeviceDescriptor,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(windows, test))]
pub(crate) enum AudioSourceMode {
    Microphone,
    System,
    Mixed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(windows, test))]
pub(crate) enum AudioEndpointKind {
    Microphone,
    SystemLoopback,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(windows, test))]
pub(crate) enum AudioEndpointState {
    Available,
    #[cfg(test)]
    PermissionDenied,
    #[cfg(test)]
    PolicyDenied,
    #[cfg(test)]
    DeviceBusy,
    #[cfg(test)]
    Unplugged,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(windows, test))]
pub(crate) enum AudioPlanError {
    MissingMicrophone,
    MissingSystemAudio,
    SourceUnavailable(AudioUnavailableReason),
    WrongEndpointKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(any(windows, test))]
pub(crate) struct AudioEndpoint {
    pub(crate) stable_id: String,
    pub(crate) label: String,
    pub(crate) is_default: bool,
    pub(crate) kind: AudioEndpointKind,
    pub(crate) state: AudioEndpointState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(windows, test))]
pub(crate) struct AudioSourceRequest<'a> {
    pub(crate) mode: AudioSourceMode,
    pub(crate) microphone_id: Option<&'a str>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(any(windows, test))]
pub(crate) struct AudioSourceSelection {
    pub(crate) mode: AudioSourceMode,
    pub(crate) microphone: Option<AudioEndpoint>,
    pub(crate) system_loopback: Option<AudioEndpoint>,
}

#[derive(Default)]
#[cfg(any(windows, test))]
struct CapabilityAccumulator {
    microphone_devices: Vec<MicrophoneDeviceDescriptor>,
    microphone: SourceAvailability,
    system: SourceAvailability,
    unavailable_reasons: Vec<AudioUnavailableReason>,
}

#[derive(Default)]
#[cfg(any(windows, test))]
struct SourceAvailability {
    has_endpoint: bool,
    supports: bool,
}

#[cfg(any(windows, test))]
impl AudioEndpoint {
    pub(crate) fn microphone(
        stable_id: &str,
        label: &str,
        is_default: bool,
        state: AudioEndpointState,
    ) -> Self {
        Self {
            stable_id: stable_id.to_owned(),
            label: label.to_owned(),
            is_default,
            kind: AudioEndpointKind::Microphone,
            state,
        }
    }

    pub(crate) fn system_loopback(stable_id: &str, label: &str, state: AudioEndpointState) -> Self {
        Self {
            stable_id: stable_id.to_owned(),
            label: label.to_owned(),
            is_default: true,
            kind: AudioEndpointKind::SystemLoopback,
            state,
        }
    }

    pub(crate) const fn unavailable_reason(&self) -> Option<AudioUnavailableReason> {
        match self.state {
            AudioEndpointState::Available => None,
            #[cfg(test)]
            AudioEndpointState::PermissionDenied => Some(AudioUnavailableReason::PermissionDenied),
            #[cfg(test)]
            AudioEndpointState::PolicyDenied => Some(AudioUnavailableReason::PolicyDenied),
            #[cfg(test)]
            AudioEndpointState::DeviceBusy => Some(AudioUnavailableReason::DeviceBusy),
            #[cfg(test)]
            AudioEndpointState::Unplugged => Some(AudioUnavailableReason::AudioSourceLost),
        }
    }
}

#[cfg(any(windows, test))]
pub(crate) fn summarize_capabilities(endpoints: &[AudioEndpoint]) -> AudioCapabilitySummary {
    let mut accumulator = CapabilityAccumulator::default();
    for endpoint in endpoints {
        accumulator.record(endpoint);
    }
    accumulator.finish()
}

#[cfg(windows)]
pub(crate) fn runtime_audio_endpoints() -> Vec<AudioEndpoint> {
    platform_audio_endpoints()
}

#[cfg(windows)]
fn platform_audio_endpoints() -> Vec<AudioEndpoint> {
    super::windows_devices::audio_endpoints().unwrap_or_default()
}

#[cfg(any(windows, test))]
impl CapabilityAccumulator {
    fn record(&mut self, endpoint: &AudioEndpoint) {
        match endpoint.kind {
            AudioEndpointKind::Microphone => self.record_microphone(endpoint),
            AudioEndpointKind::SystemLoopback => self.record_system(endpoint),
        }
        if let Some(reason) = endpoint.unavailable_reason() {
            push_unique(&mut self.unavailable_reasons, reason);
        }
    }

    fn record_microphone(&mut self, endpoint: &AudioEndpoint) {
        self.microphone.has_endpoint = true;
        let available = endpoint.state == AudioEndpointState::Available;
        self.microphone.supports |= available;
        self.microphone_devices.push(MicrophoneDeviceDescriptor {
            id: endpoint.stable_id.clone(),
            label: endpoint.label.clone(),
            is_default: endpoint.is_default,
            available,
        });
    }

    fn record_system(&mut self, endpoint: &AudioEndpoint) {
        self.system.has_endpoint = true;
        self.system.supports |= endpoint.state == AudioEndpointState::Available;
    }

    fn finish(mut self) -> AudioCapabilitySummary {
        if !self.microphone.has_endpoint || !self.system.has_endpoint {
            push_unique(
                &mut self.unavailable_reasons,
                AudioUnavailableReason::UnsupportedCapability,
            );
        }
        AudioCapabilitySummary {
            microphone_devices: self.microphone_devices,
            supports_microphone: self.microphone.supports,
            supports_system_audio: self.system.supports,
            supports_mixed_audio: self.microphone.supports && self.system.supports,
            unavailable_reasons: self.unavailable_reasons,
        }
    }
}

#[cfg(any(windows, test))]
pub(crate) fn select_sources(
    request: AudioSourceRequest<'_>,
    endpoints: &[AudioEndpoint],
) -> Result<AudioSourceSelection, AudioPlanError> {
    let microphone = match request.mode {
        AudioSourceMode::Microphone | AudioSourceMode::Mixed => {
            Some(select_microphone(request.microphone_id, endpoints)?)
        }
        AudioSourceMode::System => None,
    };
    let system_loopback = match request.mode {
        AudioSourceMode::System | AudioSourceMode::Mixed => {
            Some(select_system_loopback(endpoints)?)
        }
        AudioSourceMode::Microphone => None,
    };

    Ok(AudioSourceSelection {
        mode: request.mode,
        microphone,
        system_loopback,
    })
}

#[cfg(any(windows, test))]
fn select_microphone(
    requested_id: Option<&str>,
    endpoints: &[AudioEndpoint],
) -> Result<AudioEndpoint, AudioPlanError> {
    if let Some(requested_id) = requested_id {
        let endpoint = endpoints
            .iter()
            .find(|endpoint| {
                endpoint.kind == AudioEndpointKind::Microphone && endpoint.stable_id == requested_id
            })
            .ok_or(AudioPlanError::MissingMicrophone)?;
        return available_endpoint(endpoint, AudioPlanError::MissingMicrophone);
    }

    endpoints
        .iter()
        .filter(|endpoint| endpoint.kind == AudioEndpointKind::Microphone)
        .find(|endpoint| endpoint.is_default && endpoint.state == AudioEndpointState::Available)
        .or_else(|| {
            endpoints.iter().find(|endpoint| {
                endpoint.kind == AudioEndpointKind::Microphone
                    && endpoint.state == AudioEndpointState::Available
            })
        })
        .cloned()
        .ok_or(AudioPlanError::MissingMicrophone)
}

#[cfg(any(windows, test))]
fn select_system_loopback(endpoints: &[AudioEndpoint]) -> Result<AudioEndpoint, AudioPlanError> {
    let endpoint = endpoints
        .iter()
        .find(|endpoint| endpoint.kind == AudioEndpointKind::SystemLoopback)
        .ok_or(AudioPlanError::MissingSystemAudio)?;
    available_endpoint(endpoint, AudioPlanError::MissingSystemAudio)
}

#[cfg(any(windows, test))]
pub(crate) fn available_endpoint(
    endpoint: &AudioEndpoint,
    _missing_error: AudioPlanError,
) -> Result<AudioEndpoint, AudioPlanError> {
    if endpoint.state == AudioEndpointState::Available {
        Ok(endpoint.clone())
    } else {
        Err(AudioPlanError::SourceUnavailable(
            endpoint
                .unavailable_reason()
                .unwrap_or(AudioUnavailableReason::UnsupportedCapability),
        ))
    }
}

#[cfg(any(windows, test))]
fn push_unique(reasons: &mut Vec<AudioUnavailableReason>, reason: AudioUnavailableReason) {
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}
