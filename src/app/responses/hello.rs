//! Native-host hello, install state, and capability payloads.

mod capabilities;

use serde_json::{Value, json};

use crate::app::platform::{native_cache_path, runtime_platform};
use crate::{
    install::{
        INSTALLER_VERSION, InstallChannel, InstallPackageIntegrity, InstallRuntimeHealth,
        NATIVE_HOST_MANIFEST_VERSION, current_install_runtime_health, expected_install_identity,
    },
    protocol::{
        AppHelloMessage, CACHE_SCHEMA_VERSION, MIN_SUPPORTED_EXTENSION_VERSION,
        NativeAutostartMethod, NativeAutostartState, NativeInstallState, NativePackageIntegrity,
        NativeUpdateChannel, PROTOCOL_VERSION, SETTINGS_SCHEMA_VERSION, WireMessage,
    },
    settings::native_cache_health_from_path,
};

pub(in crate::app) use capabilities::runtime_capability_snapshot;
#[cfg(test)]
pub(in crate::app) use capabilities::{CapabilityFacts, capabilities_from_facts};

#[cfg(any(windows, test))]
pub(in crate::app) fn app_hello() -> Value {
    app_hello_for_recording_formats(&super::session::NegotiatedRecordingFormats::default())
}

pub(super) fn app_hello_for_recording_formats(
    formats: &super::session::NegotiatedRecordingFormats,
) -> Value {
    #[cfg(all(windows, not(test)))]
    let Ok(client_id) = crate::install::current_desktop_client_id() else {
        return super::outbound::operation_failed(super::outbound::OperationFailure {
            controller_lease_id: None,
            operation: crate::protocol::OperationFailedOperation::Handshake,
            operation_id: None,
            phase: "installation-identity",
            kind: crate::protocol::OperationalErrorKind::StorageUnavailable,
            message: None,
            occurred_at_epoch_ms: 1,
        });
    };
    #[cfg(any(not(windows), test))]
    let client_id = "0".repeat(64);
    json!(WireMessage::AppHello(Box::new(app_hello_message(
        client_id, formats
    ))))
}

#[cfg(test)]
pub(in crate::app) fn install_state() -> Value {
    json!(runtime_install_state())
}

fn app_hello_message(
    client_id: String,
    formats: &super::session::NegotiatedRecordingFormats,
) -> AppHelloMessage {
    use crate::protocol::{DesktopAppIdentity, DesktopSettingsAuthority};
    let mut capabilities = capabilities::handshake_capability_snapshot();
    capabilities.attachments = formats
        .cursor_trajectory
        .then(crate::protocol::trajectory::NativeTrajectoryAttachmentCapabilities::v1);
    capabilities.event_attachments = formats
        .recording_events
        .then(crate::protocol::recording_events::NativeEventAttachmentCapabilities::v1);
    AppHelloMessage {
        protocol_version: PROTOCOL_VERSION,
        settings_schema_version: SETTINGS_SCHEMA_VERSION,
        min_extension_version: MIN_SUPPORTED_EXTENSION_VERSION.to_owned(),
        supported_protocol_versions: vec![PROTOCOL_VERSION],
        supported_settings_schema_versions: vec![SETTINGS_SCHEMA_VERSION],
        app_instance_id: "sniptale-native-agent".to_owned(),
        app_identity: DesktopAppIdentity {
            client_id,
            trust_version: 1,
        },
        settings_authority: DesktopSettingsAuthority::Desktop,
        platform: runtime_platform(),
        install: runtime_install_state(),
        capabilities,
    }
}

pub(crate) fn runtime_install_state() -> NativeInstallState {
    let identity = expected_install_identity();
    let health = current_install_runtime_health();
    let cache = native_cache_health_from_path(native_cache_path());
    NativeInstallState {
        platform: runtime_platform(),
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        app_cache_schema_version: CACHE_SCHEMA_VERSION,
        installer_version: INSTALLER_VERSION.to_owned(),
        native_host_manifest_version: NATIVE_HOST_MANIFEST_VERSION.to_owned(),
        update_channel: native_update_channel(identity.channel),
        signed_binary: health.signed_binary,
        rollback_protected: cache.rollback_protected,
        autostart: NativeAutostartState {
            supported: true,
            enabled: health.autostart_enabled,
            method: NativeAutostartMethod::WindowsHkcuRun,
        },
        notarized: None,
        package_integrity: package_integrity(&health),
    }
}

const fn native_update_channel(channel: InstallChannel) -> NativeUpdateChannel {
    match channel {
        InstallChannel::Stable => NativeUpdateChannel::Stable,
        InstallChannel::Beta => NativeUpdateChannel::Beta,
        InstallChannel::Dev => NativeUpdateChannel::Dev,
    }
}

fn package_integrity(health: &InstallRuntimeHealth) -> NativePackageIntegrity {
    match health.package_integrity {
        #[cfg(any(windows, test))]
        InstallPackageIntegrity::Valid => NativePackageIntegrity::Valid,
        #[cfg(any(windows, test))]
        InstallPackageIntegrity::Invalid => NativePackageIntegrity::Invalid,
        InstallPackageIntegrity::Unknown => NativePackageIntegrity::Unknown,
    }
}
