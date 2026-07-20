use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativePlatformKind {
    Windows,
    Macos,
    Linux,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeArch {
    X64,
    Arm64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeDisplayServer {
    #[serde(rename = "wi\u{006e}32")]
    DisplayServerWin,
    Quartz,
    Wayland,
    X11,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativePackageKind {
    WixMsi,
    Pkg,
    Dmg,
    Deb,
    Rpm,
    Appimage,
    Tar,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativePlatform {
    pub(crate) kind: NativePlatformKind,
    pub(crate) version: String,
    pub(crate) arch: NativeArch,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) display_server: Option<NativeDisplayServer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) package_kind: Option<NativePackageKind>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeAutostartMethod {
    WindowsHkcuRun,
    MacosLaunchAgent,
    LinuxSystemdUser,
    LinuxXdgAutostart,
    None,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeAutostartState {
    pub(crate) supported: bool,
    pub(crate) enabled: bool,
    pub(crate) method: NativeAutostartMethod,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeUpdateChannel {
    Stable,
    Beta,
    Dev,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativePackageIntegrity {
    Valid,
    Invalid,
    Unknown,
    Unsupported,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeInstallState {
    pub(crate) platform: NativePlatform,
    pub(crate) app_version: String,
    pub(crate) app_cache_schema_version: u16,
    pub(crate) installer_version: String,
    pub(crate) native_host_manifest_version: String,
    pub(crate) update_channel: NativeUpdateChannel,
    pub(crate) signed_binary: bool,
    pub(crate) rollback_protected: bool,
    pub(crate) autostart: NativeAutostartState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) notarized: Option<bool>,
    pub(crate) package_integrity: NativePackageIntegrity,
}
