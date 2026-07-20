//! Runtime platform facts for hello/install DTOs.

use std::path::PathBuf;

use crate::{
    protocol::{
        NativeArch, NativeDisplayServer, NativePackageKind, NativePlatform, NativePlatformKind,
    },
    settings::{local_profile_file_name, native_cache_file_name},
};

pub(super) fn runtime_platform() -> NativePlatform {
    NativePlatform {
        kind: runtime_platform_kind(),
        version: runtime_platform_version(),
        arch: runtime_arch(),
        display_server: Some(runtime_display_server()),
        package_kind: Some(runtime_package_kind()),
    }
}

pub(super) fn native_cache_path() -> Option<PathBuf> {
    local_native_agent_dir().map(|dir| dir.join(native_cache_file_name()))
}

pub(super) fn local_profile_path() -> Option<PathBuf> {
    local_native_agent_dir().map(|dir| dir.join(local_profile_file_name()))
}

#[cfg(any(windows, test))]
pub(super) fn ui_preferences_path() -> Option<PathBuf> {
    use crate::settings::ui_preferences_file_name;

    local_native_agent_dir().map(|dir| dir.join(ui_preferences_file_name()))
}

#[cfg(windows)]
fn local_native_agent_dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|root| root.join("Sniptale").join("NativeAgent"))
}

#[cfg(not(windows))]
fn local_native_agent_dir() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
const fn runtime_platform_kind() -> NativePlatformKind {
    NativePlatformKind::Windows
}

#[cfg(target_os = "macos")]
const fn runtime_platform_kind() -> NativePlatformKind {
    NativePlatformKind::Macos
}

#[cfg(all(unix, not(target_os = "macos")))]
const fn runtime_platform_kind() -> NativePlatformKind {
    NativePlatformKind::Linux
}

#[cfg(windows)]
fn runtime_platform_version() -> String {
    std::env::var("OS").unwrap_or_else(|_| "unknown".to_owned())
}

#[cfg(not(windows))]
fn runtime_platform_version() -> String {
    "unknown".to_owned()
}

#[cfg(target_arch = "aarch64")]
const fn runtime_arch() -> NativeArch {
    NativeArch::Arm64
}

#[cfg(not(target_arch = "aarch64"))]
const fn runtime_arch() -> NativeArch {
    NativeArch::X64
}

#[cfg(windows)]
const fn runtime_display_server() -> NativeDisplayServer {
    NativeDisplayServer::DisplayServerWin
}

#[cfg(target_os = "macos")]
const fn runtime_display_server() -> NativeDisplayServer {
    NativeDisplayServer::Quartz
}

#[cfg(all(unix, not(target_os = "macos")))]
fn runtime_display_server() -> NativeDisplayServer {
    linux_display_server_from_env(
        std::env::var_os("WAYLAND_DISPLAY").is_some(),
        std::env::var_os("DISPLAY").is_some(),
    )
}

#[cfg(all(unix, not(target_os = "macos")))]
const fn linux_display_server_from_env(has_wayland: bool, has_x11: bool) -> NativeDisplayServer {
    if has_wayland {
        NativeDisplayServer::Wayland
    } else if has_x11 {
        NativeDisplayServer::X11
    } else {
        NativeDisplayServer::Unknown
    }
}

#[cfg(windows)]
const fn runtime_package_kind() -> NativePackageKind {
    NativePackageKind::WixMsi
}

#[cfg(not(windows))]
const fn runtime_package_kind() -> NativePackageKind {
    NativePackageKind::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(windows)]
    fn platform_fact_tracks_windows_target() {
        let platform = runtime_platform();

        assert_eq!(platform.kind, NativePlatformKind::Windows);
        assert_eq!(
            platform.display_server,
            Some(NativeDisplayServer::DisplayServerWin)
        );
        assert_eq!(platform.package_kind, Some(NativePackageKind::WixMsi));
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn platform_fact_tracks_linux_target_without_windows_defaults() {
        let platform = runtime_platform();

        assert_eq!(platform.kind, NativePlatformKind::Linux);
        assert_ne!(
            platform.display_server,
            Some(NativeDisplayServer::DisplayServerWin)
        );
        assert_eq!(platform.package_kind, Some(NativePackageKind::Unknown));
    }

    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn linux_display_server_prefers_wayland_then_x11_then_unknown() {
        assert_eq!(
            linux_display_server_from_env(true, true),
            NativeDisplayServer::Wayland
        );
        assert_eq!(
            linux_display_server_from_env(false, true),
            NativeDisplayServer::X11
        );
        assert_eq!(
            linux_display_server_from_env(false, false),
            NativeDisplayServer::Unknown
        );
    }
}
