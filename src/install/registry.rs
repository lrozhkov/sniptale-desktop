//! Native Messaging manifests and autostart registration checks.

mod manifest;

use super::identity::expected_install_identity;

#[cfg(test)]
pub(crate) use manifest::{MAX_NATIVE_HOST_MANIFEST_BYTES, parse_manifest_json};
pub(crate) use manifest::{
    ManifestError, NATIVE_HOST_MANIFEST_VERSION, NativeHostManifest, read_manifest_file,
    validate_manifest,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InstallChannel {
    Stable,
    Beta,
    Dev,
}

pub(crate) const INSTALLER_VERSION: &str = "0.1.0";
pub(crate) const DEFAULT_ALLOWED_EXTENSION_ORIGIN: &str =
    "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/";

pub(crate) fn expected_install_channel() -> InstallChannel {
    match option_env!("SNIPTALE_INSTALL_CHANNEL") {
        Some("beta") => InstallChannel::Beta,
        Some("dev") => InstallChannel::Dev,
        _ => InstallChannel::Stable,
    }
}

pub(crate) fn expected_allowed_extension_origins() -> [&'static str; 1] {
    [
        option_env!("SNIPTALE_ALLOWED_EXTENSION_ORIGIN")
            .unwrap_or(DEFAULT_ALLOWED_EXTENSION_ORIGIN),
    ]
}

pub(crate) const fn expected_install_id(channel: InstallChannel) -> &'static str {
    match channel {
        InstallChannel::Stable => "install-stable",
        InstallChannel::Beta => "install-beta",
        InstallChannel::Dev => "install-dev",
    }
}

pub(crate) const fn expected_host_name(channel: InstallChannel) -> &'static str {
    match channel {
        InstallChannel::Stable => "com.sniptale.native_host",
        InstallChannel::Beta => "com.sniptale.beta.native_host",
        InstallChannel::Dev => "com.sniptale.dev.native_host",
    }
}

pub(crate) fn allowed_extension_origin(origin: &str) -> bool {
    expected_install_identity().allows_extension_origin(origin)
}
