use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ScreenshotMode {
    Screen,
    ActiveWindow,
    AllScreens,
    Region,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum VideoMode {
    Screen,
    ActiveWindow,
    Region,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ContainerCodec {
    Mp4,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum VideoCodec {
    H264,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AudioCodec {
    Aac,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeCapabilityUnavailableReason {
    OsTooOld,
    EncoderMissing,
    DeviceBusy,
    PermissionDenied,
    PolicyDenied,
    UnsupportedBrowser,
    UnsupportedProfile,
    UnsupportedPlatform,
    UnsupportedDisplayServer,
    PlatformPermissionRequired,
    NotarizationInvalid,
    ProtectedContent,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeMicrophoneDevice {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) is_default: bool,
    pub(crate) available: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeCaptureCapabilities {
    pub(crate) screenshot_modes: Vec<ScreenshotMode>,
    pub(crate) video_modes: Vec<VideoMode>,
    pub(crate) supports_freeze_region_selection: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeCodecCapabilities {
    pub(crate) containers: Vec<ContainerCodec>,
    pub(crate) video: Vec<VideoCodec>,
    pub(crate) audio: Vec<AudioCodec>,
    pub(crate) hardware_encoder_available: bool,
    pub(crate) unavailable_reasons: Vec<NativeCapabilityUnavailableReason>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeAudioCapabilities {
    pub(crate) microphone_devices: Vec<NativeMicrophoneDevice>,
    pub(crate) supports_microphone: bool,
    pub(crate) supports_system_audio: bool,
    pub(crate) supports_mixed_audio: bool,
    pub(crate) unavailable_reasons: Vec<NativeCapabilityUnavailableReason>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_field_names,
    reason = "wire DTO field names mirror the extension capability contract"
)]
pub(crate) struct NativeCapabilityLimits {
    pub(crate) max_chunk_bytes: u64,
    pub(crate) max_recording_bytes: u64,
    pub(crate) max_screenshot_bytes: u64,
    pub(crate) max_fps: u16,
    pub(crate) max_width: u32,
    pub(crate) max_height: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct NativeAppCapabilities {
    pub(crate) capture: NativeCaptureCapabilities,
    pub(crate) codecs: NativeCodecCapabilities,
    pub(crate) audio: NativeAudioCapabilities,
    pub(crate) limits: NativeCapabilityLimits,
}
