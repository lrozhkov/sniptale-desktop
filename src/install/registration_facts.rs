//! Runtime install registration fact collection.
mod registry_locations;

use std::{env, path::PathBuf};

use super::{
    identity::{InstallRuntimeIdentity, expected_install_identity},
    registry::{InstallChannel, ManifestError, NativeHostManifest, read_manifest_file},
    signature::{BinarySignatureReport, current_binary_signature_report},
};
use registry_locations::native_host_registry_locations;
pub(crate) use registry_locations::{
    BrowserFamily, NativeHostRegistryEntry, NativeHostRegistryLocation, RegistryHive,
    native_host_registry_subkey,
};

pub(crate) const NATIVE_HOST_MANIFEST_FILE_NAME: &str = "native-host.json";
#[cfg(windows)]
pub(crate) const AUTOSTART_VALUE_NAME: &str = "SniptaleNativeAgent";
#[cfg(windows)]
const AUTOSTART_RUN_SUBKEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CurrentRegistrationFacts {
    pub(crate) manifest: Result<NativeHostManifest, ManifestError>,
    pub(crate) shadow_manifests: Vec<Result<NativeHostManifest, ManifestError>>,
    pub(crate) expected_channel: InstallChannel,
    pub(crate) expected_executable_path: String,
    pub(crate) expected_allowed_origins: Vec<&'static str>,
    pub(crate) signature_report: BinarySignatureReport,
    pub(crate) autostart_command: RuntimeAutostartFact,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RuntimeAutostartFact {
    #[cfg_attr(not(windows), allow(dead_code))]
    Command(Option<String>),
    Unknown,
}

pub(crate) fn current_registration_facts() -> CurrentRegistrationFacts {
    let identity = expected_install_identity();
    let executable_path = current_executable_path();
    let expected_manifest_path = executable_path
        .as_deref()
        .and_then(expected_native_host_manifest_path);
    let registry_entries = read_native_host_registry_entries(identity);
    let primary_manifest = primary_manifest_fact(
        &registry_entries,
        expected_manifest_path.as_ref(),
        identity.channel,
    );
    let shadow_manifests = shadow_manifest_facts(
        &registry_entries,
        expected_manifest_path.as_ref(),
        identity.channel,
    );
    let expected_executable_path = executable_path.unwrap_or_default();
    let signature_report = current_binary_signature_report(&expected_executable_path);

    CurrentRegistrationFacts {
        manifest: primary_manifest,
        shadow_manifests,
        expected_channel: identity.channel,
        expected_executable_path,
        expected_allowed_origins: identity.allowed_origins.to_vec(),
        signature_report,
        autostart_command: read_autostart_command(),
    }
}

/// Returns the executable selected by the primary Native Messaging registration only when it
/// matches the fixed per-user install location and satisfies release or explicit dev trust.
#[cfg(windows)]
pub(crate) fn trusted_native_host_executable_path() -> Option<PathBuf> {
    let identity = expected_install_identity();
    let expected = PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
        .join("Sniptale")
        .join("NativeAgent")
        .join("sniptale-native-agent.exe");
    let entries = read_native_host_registry_entries(identity);
    let entry = entries.into_iter().find_map(|entry| match entry {
        Ok(entry) if is_primary_manifest_location(entry.location) => Some(entry),
        _ => None,
    })?;
    let manifest = read_manifest_file(&entry.manifest_path, identity.channel).ok()?;
    let expected_path = expected.to_string_lossy();
    let signature = current_binary_signature_report(&expected_path);
    trusted_registered_host_candidate(
        &manifest,
        &expected_path,
        identity,
        signature.status,
        crate::install::dev_local_browser_trust_enabled(),
    )
    .then_some(expected)
}

#[cfg(any(windows, test))]
pub(crate) fn trusted_registered_host_candidate(
    manifest: &NativeHostManifest,
    expected_path: &str,
    identity: InstallRuntimeIdentity,
    signature: super::signature::SignatureStatus,
    dev_local_trust: bool,
) -> bool {
    super::registry::validate_manifest(
        manifest,
        identity.channel,
        expected_path,
        &identity.allowed_origins,
    )
    .is_ok()
        && (signature == super::signature::SignatureStatus::Valid
            || (dev_local_trust && signature == super::signature::SignatureStatus::Missing))
}

pub(crate) fn expected_native_host_manifest_path(executable_path: &str) -> Option<PathBuf> {
    let separator = executable_path
        .rfind(['\\', '/'])
        .map(|index| (index, executable_path.as_bytes()[index] as char))?;
    let (index, separator) = separator;
    let parent = &executable_path[..index];
    Some(PathBuf::from(format!(
        "{parent}{separator}{NATIVE_HOST_MANIFEST_FILE_NAME}"
    )))
}

fn current_executable_path() -> Option<String> {
    env::current_exe()
        .ok()
        .and_then(|path| path.into_os_string().into_string().ok())
}

pub(super) fn primary_manifest_fact(
    registry_entries: &[Result<NativeHostRegistryEntry, ManifestError>],
    expected_manifest_path: Option<&PathBuf>,
    expected_channel: InstallChannel,
) -> Result<NativeHostManifest, ManifestError> {
    registry_entries
        .iter()
        .find_map(|entry| match entry {
            Ok(entry) if is_primary_manifest_location(entry.location) => Some(entry),
            _ => None,
        })
        .map_or(Err(ManifestError::MissingManifest), |entry| {
            read_manifest_from_registry_entry(entry, expected_manifest_path, expected_channel)
        })
}

pub(super) fn shadow_manifest_facts(
    registry_entries: &[Result<NativeHostRegistryEntry, ManifestError>],
    expected_manifest_path: Option<&PathBuf>,
    expected_channel: InstallChannel,
) -> Vec<Result<NativeHostManifest, ManifestError>> {
    registry_entries
        .iter()
        .filter_map(|entry| match entry {
            Ok(entry) if !is_primary_manifest_location(entry.location) => Some(
                read_manifest_from_registry_entry(entry, expected_manifest_path, expected_channel),
            ),
            Err(error) => Some(Err(*error)),
            _ => None,
        })
        .collect()
}

fn read_manifest_from_registry_entry(
    entry: &NativeHostRegistryEntry,
    expected_manifest_path: Option<&PathBuf>,
    expected_channel: InstallChannel,
) -> Result<NativeHostManifest, ManifestError> {
    if expected_manifest_path.is_some_and(|expected_path| &entry.manifest_path != expected_path) {
        return Err(ManifestError::PathDrift);
    }
    read_manifest_file(&entry.manifest_path, expected_channel)
}

const fn is_primary_manifest_location(location: NativeHostRegistryLocation) -> bool {
    matches!(
        location,
        NativeHostRegistryLocation {
            hive: RegistryHive::CurrentUser,
            browser: BrowserFamily::Chrome,
        }
    )
}

fn read_native_host_registry_entries(
    identity: InstallRuntimeIdentity,
) -> Vec<Result<NativeHostRegistryEntry, ManifestError>> {
    native_host_registry_locations()
        .into_iter()
        .filter_map(|location| {
            let subkey = native_host_registry_subkey(location.browser, identity.host_name);
            read_registry_string(location.hive, &subkey, None).map(|result| {
                result.map(|manifest_path| NativeHostRegistryEntry {
                    location,
                    manifest_path: PathBuf::from(manifest_path),
                })
            })
        })
        .collect()
}

fn read_autostart_command() -> RuntimeAutostartFact {
    read_platform_autostart_command()
}

#[cfg(windows)]
fn read_registry_string(
    hive: RegistryHive,
    subkey: &str,
    value_name: Option<&str>,
) -> Option<Result<String, ManifestError>> {
    use super::windows::registry::{WindowsRegistryError, read_string_value};

    match read_string_value(hive, subkey, value_name) {
        Ok(Some(value)) => Some(Ok(value)),
        Ok(None) => None,
        Err(WindowsRegistryError::UnsupportedType | WindowsRegistryError::InvalidUtf16) => {
            Some(Err(ManifestError::InvalidJson))
        }
        Err(WindowsRegistryError::OpenFailed | WindowsRegistryError::QueryFailed) => {
            Some(Err(ManifestError::UnreadableManifest))
        }
    }
}

#[cfg(not(windows))]
fn read_registry_string(
    _hive: RegistryHive,
    _subkey: &str,
    _value_name: Option<&str>,
) -> Option<Result<String, ManifestError>> {
    None
}

#[cfg(windows)]
fn read_platform_autostart_command() -> RuntimeAutostartFact {
    use super::windows::registry::read_string_value;

    match read_string_value(
        RegistryHive::CurrentUser,
        AUTOSTART_RUN_SUBKEY,
        Some(AUTOSTART_VALUE_NAME),
    ) {
        Ok(Some(command)) => RuntimeAutostartFact::Command(Some(command)),
        Ok(None) => RuntimeAutostartFact::Command(None),
        Err(_) => RuntimeAutostartFact::Unknown,
    }
}

#[cfg(not(windows))]
fn read_platform_autostart_command() -> RuntimeAutostartFact {
    RuntimeAutostartFact::Unknown
}
