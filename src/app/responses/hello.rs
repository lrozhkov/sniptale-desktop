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

pub(in crate::app) fn app_hello() -> Value {
    json!(WireMessage::AppHello(Box::new(app_hello_message())))
}

#[cfg(test)]
pub(in crate::app) fn install_state() -> Value {
    json!(runtime_install_state())
}

fn app_hello_message() -> AppHelloMessage {
    AppHelloMessage {
        protocol_version: PROTOCOL_VERSION,
        settings_schema_version: SETTINGS_SCHEMA_VERSION,
        min_extension_version: MIN_SUPPORTED_EXTENSION_VERSION.to_owned(),
        supported_protocol_versions: vec![PROTOCOL_VERSION],
        supported_settings_schema_versions: vec![SETTINGS_SCHEMA_VERSION],
        app_instance_id: "sniptale-native-agent".to_owned(),
        platform: runtime_platform(),
        install: runtime_install_state(),
        capabilities: capabilities::handshake_capability_snapshot(),
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
