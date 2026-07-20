//! Platform-neutral wire DTOs, parsing, versions, and typed errors.

mod capabilities;
mod control;
mod errors;
mod media;
#[cfg(any(windows, test))]
mod operation_route;
mod parse;
mod platform;
pub(crate) mod recording;
#[cfg(test)]
mod tests;
mod validate;
mod validation;
mod versions;
mod wire;

pub(crate) use capabilities::{
    AudioCodec, ContainerCodec, NativeAppCapabilities, NativeAudioCapabilities,
    NativeCapabilityLimits, NativeCapabilityUnavailableReason, NativeCaptureCapabilities,
    NativeCodecCapabilities, NativeMicrophoneDevice, ScreenshotMode, VideoCodec, VideoMode,
};
#[cfg(test)]
pub(crate) use control::controller::BrowserFamily;
pub(crate) use control::controller::{
    AppControllerLeaseMessage, ControllerAcquireReason, ControllerLeaseStatus,
    ExtensionControllerAcquireMessage, NativeControllerIdentity,
};
pub(crate) use control::handshake::{AppHelloMessage, ExtensionHelloMessage};
#[cfg(any(windows, test))]
pub(crate) use control::media_commands::AppCommandAcceptedMessage;
#[cfg(any(windows, test))]
pub(crate) use control::media_commands::RecordingControl;
pub(crate) use control::media_commands::{
    AcceptedOperation, ExtensionRecordingControlCommandMessage,
    ExtensionRecordingStartCommandMessage, ExtensionScreenshotCaptureCommandMessage,
};
pub(crate) use control::settings::{
    AppSettingsAcceptedMessage, ExtensionSettingsSyncMessage, NativeEffectiveSettings,
    NativeSettingsSnapshot,
};
pub(crate) use control::status::{AppPongMessage, NativeAppStatus};
pub(crate) use control::tray::ExtensionIntentActionId;
#[cfg(test)]
pub(crate) use control::tray::NativeTrayActionDescriptor;
#[cfg(any(windows, test))]
pub(crate) use control::tray::{
    AppOpenSettingsRequestedMessage, AppTrayActionRequestedMessage, OpenSettingsSection,
};
pub(crate) use control::tray::{NativeTrayActionKind, NativeTrayActionRegistry};
pub(crate) use errors::ParseError;
#[cfg(test)]
pub(crate) use media::failure::NativeAppErrorCode;
pub(crate) use media::failure::{
    AppOperationFailedMessage, NativeAppError, OperationFailedOperation,
};
pub(crate) use media::operation_error::OperationalErrorKind;
#[cfg(any(windows, test))]
pub(crate) use media::recording::{
    AppRecordingChunkMessage, AppRecordingProgressMessage, AppRecordingStartedMessage,
    NativeRecordingTelemetrySnapshot, NativeRecordingTimebase, NativeRecordingTimebaseUnits,
    RecordingProgressStatus,
};
pub(crate) use media::recording::{AppRecordingStoppedMessage, RecordingMimeType};
#[cfg(any(windows, test))]
pub(crate) use media::screenshot::{
    AppScreenshotChunkMessage, AppScreenshotCommitMessage, AppScreenshotStartMessage,
    ScreenshotMimeType,
};
#[cfg(any(windows, test))]
pub(crate) use operation_route::{DeliveryRoute, OperationAuthority};
pub(crate) use parse::{parse_wire_message, parse_wire_message_with_limit};
pub(crate) use platform::{
    NativeArch, NativeAutostartMethod, NativeAutostartState, NativeDisplayServer,
    NativeInstallState, NativePackageIntegrity, NativePackageKind, NativePlatform,
    NativePlatformKind, NativeUpdateChannel,
};
#[cfg(any(windows, test))]
pub(crate) use recording::NativeEffectiveQualitySettings;
#[cfg(test)]
pub(crate) use recording::NativeRecordingRegion;
#[cfg(any(windows, test))]
pub(crate) use recording::NativeRecordingSource;
#[cfg(any(windows, test))]
pub(crate) use recording::NativeRequestedQualitySettings;
#[cfg(test)]
pub(crate) use recording::{
    NativeAudioSourceMode, NativeEffectiveEncoder, NativeVideoFrameRate, NativeVideoQuality,
};
#[cfg(test)]
pub(crate) use versions::EXTENSION_TO_HOST_CHROMIUM_MAX_MESSAGE_BYTES;
pub(crate) use versions::{
    CACHE_SCHEMA_VERSION, CHUNK_RAW_BYTES, EXTENSION_TO_HOST_MAX_MESSAGE_BYTES,
    HOST_TO_EXTENSION_MAX_MESSAGE_BYTES, MIN_SUPPORTED_EXTENSION_VERSION, PROTOCOL_VERSION,
    RECORDING_MAX_TOTAL_BYTES, SCREENSHOT_MAX_TOTAL_BYTES, SETTINGS_SCHEMA_VERSION,
};
#[cfg(any(windows, test))]
pub(crate) use versions::{CHUNK_MAX_BASE64_CHARS, MESSAGE_TARGET_MAX_JSON_UTF8_BYTES};
pub(crate) use wire::WireMessage;
