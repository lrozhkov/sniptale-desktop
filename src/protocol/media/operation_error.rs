//! Stable operation failure taxonomy for native app diagnostics.

use super::failure::NativeAppErrorCode as Code;

impl Code {
    #[rustfmt::skip]
    pub(crate) const ALL: &'static [Self] = &[
        Self::MalformedMessage, Self::UnsupportedVersion, Self::IncompatibleSettings,
        Self::StaleControllerLease, Self::DuplicateCommand, Self::UnsupportedCapability,
        Self::QuotaExceeded, Self::HashMismatch, Self::DuplicateOrReplay, Self::OversizedPayload,
        Self::StorageFailed, Self::InvalidSelection, Self::UserCancelled, Self::Unknown,
        Self::OsTooOld, Self::EncoderMissing, Self::DeviceBusy, Self::PermissionDenied,
        Self::PolicyDenied, Self::UnsupportedBrowser, Self::UnsupportedProfile,
        Self::UnsupportedPlatform, Self::UnsupportedDisplayServer, Self::PlatformPermissionRequired,
        Self::NotarizationInvalid, Self::ProtectedContent, Self::AppUpgradeRequired,
        Self::ExtensionUpgradeRequired, Self::IncompatibleProtocol, Self::RepairRequired,
        Self::DiskPressure, Self::NativeMessageTooLarge, Self::CaptureSourceLost,
        Self::AudioSourceLost, Self::EncoderFailed, Self::NativeFinalizationFailed,
        Self::CommandDispatchFailed, Self::InstallIdentityDrift, Self::PipeSecretUnavailable,
        Self::PipeSecretCorrupt, Self::PipeConnectTimeout, Self::PipeBusy,
        Self::ResidentStartupFailed, Self::AuthRejected, Self::ProtectedMediaFailure,
        Self::ChunkOutOfBounds, Self::TransferExpired, Self::SerializationContractFailed,
        Self::LoggerStartupFailed, Self::QueueBackpressure, Self::HotkeyRuntimeFailed,
        Self::TrayRuntimeFailed, Self::RecordingRuntimeFailed, Self::ScreenshotCaptureFailed,
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OperationalErrorKind {
    MalformedMessage,
    HandshakeIncompatibleProtocol,
    HandshakeIncompatibleSettings,
    AppUpgradeRequired,
    ExtensionUpgradeRequired,
    StaleControllerLease,
    CommandSettingsRevisionMismatch,
    SettingsStateUnavailable,
    SettingsSchemaVersionIncompatible,
    SettingsSnapshotInvalid,
    TrayRegistryInvalid,
    HotkeyBindingsInvalid,
    HotkeyRuntimeFailed,
    TrayRuntimeFailed,
    UnsupportedCapability,
    DiskPressure,
    ScreenshotSelectionCancelled,
    ScreenshotInvalidSelection,
    ScreenshotCaptureFailed,
    RecordingRuntimeFailed,
    CommandDispatchFailed,
    StorageUnavailable,
    TransferNotPending,
    TransferChannelUnsupported,
    RecordingRecoveryUnavailable,
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
    EncoderUnavailable,
    SerializationContractFailure,
    LoggerStartupFailed,
    QueueBackpressure,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OperationalErrorDescriptor {
    pub(crate) catalog_key: &'static str,
    pub(crate) public_code: Code,
    pub(crate) default_message: &'static str,
    pub(crate) recoverable: bool,
}

impl OperationalErrorKind {
    #[rustfmt::skip]
    pub(crate) const ALL: &'static [Self] = &[
        Self::MalformedMessage, Self::HandshakeIncompatibleProtocol,
        Self::HandshakeIncompatibleSettings, Self::AppUpgradeRequired,
        Self::ExtensionUpgradeRequired, Self::StaleControllerLease,
        Self::CommandSettingsRevisionMismatch, Self::SettingsStateUnavailable,
        Self::SettingsSchemaVersionIncompatible, Self::SettingsSnapshotInvalid,
        Self::TrayRegistryInvalid, Self::HotkeyBindingsInvalid, Self::HotkeyRuntimeFailed,
        Self::TrayRuntimeFailed, Self::UnsupportedCapability, Self::DiskPressure,
        Self::ScreenshotSelectionCancelled, Self::ScreenshotInvalidSelection,
        Self::ScreenshotCaptureFailed, Self::RecordingRuntimeFailed, Self::CommandDispatchFailed,
        Self::StorageUnavailable, Self::TransferNotPending, Self::TransferChannelUnsupported,
        Self::RecordingRecoveryUnavailable, Self::InstallIdentityDrift, Self::PipeSecretUnavailable,
        Self::PipeSecretCorrupt, Self::PipeConnectTimeout, Self::PipeBusy,
        Self::ResidentStartupFailed, Self::AuthRejected, Self::ProtectedMediaFailure,
        Self::ChunkOutOfBounds, Self::TransferExpired, Self::EncoderUnavailable,
        Self::SerializationContractFailure, Self::LoggerStartupFailed, Self::QueueBackpressure,
    ];

    #[rustfmt::skip]
    pub(crate) const fn descriptor(self) -> OperationalErrorDescriptor {
        match self {
            Self::MalformedMessage => d("malformed-message", Code::MalformedMessage, "message is malformed", true),
            Self::HandshakeIncompatibleProtocol => d("handshake-incompatible-protocol", Code::IncompatibleProtocol, "no mutually supported protocol version", false),
            Self::HandshakeIncompatibleSettings => d("handshake-incompatible-settings", Code::IncompatibleSettings, "no mutually supported settings schema version", false),
            Self::AppUpgradeRequired => d("app-upgrade-required", Code::AppUpgradeRequired, "native app version is below extension minimum", true),
            Self::ExtensionUpgradeRequired => d("extension-upgrade-required", Code::ExtensionUpgradeRequired, "extension version is below native app minimum", true),
            Self::StaleControllerLease => d("stale-controller-lease", Code::StaleControllerLease, "controller lease is not active", true),
            Self::CommandSettingsRevisionMismatch => d("command-settings-revision-mismatch", Code::IncompatibleSettings, "command settings revision is not accepted", true),
            Self::SettingsStateUnavailable => d("settings-state-unavailable", Code::StorageFailed, "settings state is unavailable", true),
            Self::SettingsSchemaVersionIncompatible => d("settings-schema-version-incompatible", Code::IncompatibleSettings, "settings schema version is incompatible", true),
            Self::SettingsSnapshotInvalid => d("settings-snapshot-invalid", Code::IncompatibleSettings, "settings snapshot is invalid", true),
            Self::TrayRegistryInvalid => d("tray-actions-invalid", Code::IncompatibleSettings, "tray actions are invalid", true),
            Self::HotkeyBindingsInvalid => d("hotkey-bindings-invalid", Code::IncompatibleSettings, "hotkey bindings are invalid", true),
            Self::HotkeyRuntimeFailed => d("hotkey-runtime-failed", Code::HotkeyRuntimeFailed, "hotkey registration could not be applied", true),
            Self::TrayRuntimeFailed => d("tray-runtime-failed", Code::TrayRuntimeFailed, "tray actions could not be applied", true),
            Self::UnsupportedCapability => d("unsupported-capability", Code::UnsupportedCapability, "native capture pipeline is unavailable", false),
            Self::DiskPressure => d("disk-pressure", Code::DiskPressure, "not enough free disk space for recording", true),
            Self::ScreenshotSelectionCancelled => d("screenshot-selection-cancelled", Code::UserCancelled, "region selection was cancelled", true),
            Self::ScreenshotInvalidSelection => d("screenshot-invalid-selection", Code::InvalidSelection, "region selection is invalid", true),
            Self::ScreenshotCaptureFailed => d("screenshot-capture-failed", Code::ScreenshotCaptureFailed, "screenshot capture failed", true),
            Self::RecordingRuntimeFailed => d("recording-runtime-failed", Code::RecordingRuntimeFailed, "recording runtime failed", true),
            Self::CommandDispatchFailed => d("command-dispatch-failed", Code::CommandDispatchFailed, "native command dispatch failed", true),
            Self::StorageUnavailable => d("storage-unavailable", Code::StorageFailed, "pending media transfer state is unavailable", true),
            Self::TransferNotPending => d("transfer-not-pending", Code::DuplicateOrReplay, "media transfer is not pending", true),
            Self::TransferChannelUnsupported => d("transfer-channel-unsupported", Code::UnsupportedCapability, "media bytes are not available on this native transport", true),
            Self::RecordingRecoveryUnavailable => d("recording-recovery-unavailable", Code::StorageFailed, "pending recording recovery is unavailable", true),
            Self::InstallIdentityDrift => d("install-identity-drift", Code::InstallIdentityDrift, "installed native host identity does not match registration", true),
            Self::PipeSecretUnavailable => d("pipe-secret-unavailable", Code::PipeSecretUnavailable, "resident pipe authentication secret is unavailable", true),
            Self::PipeSecretCorrupt => d("pipe-secret-corrupt", Code::PipeSecretCorrupt, "resident pipe authentication secret is corrupt", true),
            Self::PipeConnectTimeout => d("pipe-connect-timeout", Code::PipeConnectTimeout, "resident pipe connection timed out", true),
            Self::PipeBusy => d("pipe-busy", Code::PipeBusy, "resident pipe is busy", true),
            Self::ResidentStartupFailed => d("resident-startup-failed", Code::ResidentStartupFailed, "resident agent startup failed", true),
            Self::AuthRejected => d("auth-rejected", Code::AuthRejected, "resident pipe authentication was rejected", true),
            Self::ProtectedMediaFailure => d("protected-media-failure", Code::ProtectedContent, "protected media cannot be captured", true),
            Self::ChunkOutOfBounds => d("chunk-out-of-bounds", Code::ChunkOutOfBounds, "requested media chunk is out of bounds", true),
            Self::TransferExpired => d("transfer-expired", Code::TransferExpired, "media transfer expired", true),
            Self::EncoderUnavailable => d("encoder-unavailable", Code::EncoderMissing, "media encoder is unavailable", false),
            Self::SerializationContractFailure => d("serialization-contract-failure", Code::SerializationContractFailed, "native message serialization failed", true),
            Self::LoggerStartupFailed => d("logger-startup-failed", Code::LoggerStartupFailed, "process logger startup failed", true),
            Self::QueueBackpressure => d("queue-backpressure", Code::QueueBackpressure, "native command queue is saturated", true),
        }
    }

    pub(crate) fn is_cataloged(self) -> bool {
        Self::ALL.contains(&self)
    }

    pub(crate) fn has_cataloged_public_code(self) -> bool {
        Code::ALL.contains(&self.descriptor().public_code)
    }
}

const fn d(
    catalog_key: &'static str,
    public_code: Code,
    default_message: &'static str,
    recoverable: bool,
) -> OperationalErrorDescriptor {
    OperationalErrorDescriptor {
        catalog_key,
        public_code,
        default_message,
        recoverable,
    }
}
