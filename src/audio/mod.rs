//! WASAPI endpoint discovery, capture, loopback, mixing, resampling, and sync.

mod capabilities;
#[cfg(any(windows, test))]
mod capture_timeline;
mod devices;
#[cfg(any(windows, test))]
mod loopback;
#[cfg(any(windows, test))]
mod mixer;
#[cfg(any(windows, test))]
mod pcm_mix;
#[cfg(any(windows, test))]
mod pcm_source;
#[cfg(test)]
mod tests;
#[cfg(any(windows, test))]
mod wasapi_capture;
#[cfg(windows)]
mod windows_devices;
#[cfg(windows)]
mod windows_pcm_capture;

#[cfg(any(windows, test))]
pub(crate) use capabilities::MicrophoneDeviceDescriptor;
pub(crate) use capabilities::{
    AudioCapabilitySummary, AudioUnavailableReason, runtime_audio_capability_summary,
};
#[cfg(test)]
pub(crate) use devices::AudioEndpointState;
#[cfg(windows)]
pub(crate) use devices::runtime_audio_endpoints;
#[cfg(any(windows, test))]
pub(crate) use devices::{
    AudioEndpoint, AudioPlanError, AudioSourceMode, AudioSourceRequest, AudioSourceSelection,
    select_sources,
};
#[cfg(any(windows, test))]
pub(crate) use loopback::system_loopback_input_spec;
#[cfg(test)]
pub(crate) use mixer::AudioInputKind;
#[cfg(any(windows, test))]
pub(crate) use mixer::{AudioInputSpec, MixPlan, MixPlanError, PcmFormat, plan_mix};
#[cfg(test)]
pub(crate) use pcm_mix::{MixedPcmChunk, PcmMixError};
#[cfg(windows)]
pub(crate) use pcm_source::PcmMixPipeline;
#[cfg(any(windows, test))]
pub(crate) use pcm_source::{MixedPcmPoll, MixedPcmSource, PcmSourceError};
#[cfg(any(windows, test))]
pub(crate) use wasapi_capture::microphone_input_spec;
#[cfg(windows)]
pub(crate) use windows_pcm_capture::{WindowsWasapiPcmError, WindowsWasapiPcmSource};
