//! Media operation failure protocol DTOs.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeRejectReason {
    MalformedMessage,
    UnsupportedVersion,
    IncompatibleSettings,
    StaleControllerLease,
    DuplicateCommand,
    UnsupportedCapability,
    QuotaExceeded,
    HashMismatch,
    DuplicateOrReplay,
    OversizedPayload,
    StorageFailed,
    InvalidSelection,
    UserCancelled,
    Unknown,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeAppErrorCode {
    MalformedMessage,
    UnsupportedVersion,
    IncompatibleSettings,
    StaleControllerLease,
    DuplicateCommand,
    UnsupportedCapability,
    QuotaExceeded,
    HashMismatch,
    DuplicateOrReplay,
    OversizedPayload,
    StorageFailed,
    InvalidSelection,
    UserCancelled,
    Unknown,
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
    AppUpgradeRequired,
    ExtensionUpgradeRequired,
    IncompatibleProtocol,
    RepairRequired,
    DiskPressure,
    NativeMessageTooLarge,
    CaptureSourceLost,
    AudioSourceLost,
    EncoderFailed,
    NativeFinalizationFailed,
    CommandDispatchFailed,
    InstallIdentityDrift,
    PipeSecretUnavailable,
    PipeSecretCorrupt,
    PipeConnectTimeout,
    PipeBusy,
    ResidentStartupFailed,
    AuthRejected,
    ProtectedMediaFailure,
    ChunkOutOfBounds,
    TransferExpired,
    SerializationContractFailed,
    LoggerStartupFailed,
    QueueBackpressure,
    HotkeyRuntimeFailed,
    TrayRuntimeFailed,
    RecordingRuntimeFailed,
    ScreenshotCaptureFailed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) struct NativeAppError {
    pub(crate) code: NativeAppErrorCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) message: Option<String>,
    pub(crate) recoverable: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum OperationFailedOperation {
    Handshake,
    SettingsSync,
    TrayAction,
    OpenSettings,
    Command,
    Screenshot,
    Recording,
    Transfer,
    Telemetry,
    InstallHealth,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppOperationFailedMessage {
    pub(crate) protocol_version: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) controller_lease_id: Option<String>,
    pub(crate) operation: OperationFailedOperation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) operation_id: Option<String>,
    pub(crate) phase: String,
    pub(crate) error: NativeAppError,
    pub(crate) occurred_at_epoch_ms: u64,
}
