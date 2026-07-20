use std::{fmt, fs, io, path::Path};

use serde::Deserialize;

use super::{InstallChannel, expected_host_name};

pub(crate) const NATIVE_HOST_MANIFEST_VERSION: &str = "0.1.0";
pub(crate) const MAX_NATIVE_HOST_MANIFEST_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativeHostManifest {
    pub(crate) host_name: String,
    pub(crate) manifest_version: String,
    pub(crate) allowed_origins: Vec<String>,
    pub(crate) path: String,
    pub(crate) channel: InstallChannel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManifestError {
    MissingManifest,
    UnreadableManifest,
    OversizeManifest,
    InvalidJson,
    UnsupportedManifestType,
    ManifestVersionDrift,
    InvalidHostName,
    InvalidAllowedOrigin,
    MissingAllowedOrigin,
    UnexpectedAllowedOrigin,
    WrongChannel,
    PathDrift,
}

#[derive(Deserialize)]
struct RawNativeHostManifest {
    name: String,
    #[serde(rename = "sniptale_native_host_manifest_version")]
    sniptale_native_host_manifest_version: String,
    path: String,
    #[serde(rename = "type")]
    manifest_type: String,
    allowed_origins: Vec<String>,
}

pub(crate) fn read_manifest_file(
    path: &Path,
    expected_channel: InstallChannel,
) -> Result<NativeHostManifest, ManifestError> {
    let payload = fs::read(path).map_err(|error| read_error_to_manifest_error(&error))?;
    parse_manifest_json(&payload, expected_channel)
}

pub(crate) fn parse_manifest_json(
    payload: &[u8],
    expected_channel: InstallChannel,
) -> Result<NativeHostManifest, ManifestError> {
    if payload.len() > MAX_NATIVE_HOST_MANIFEST_BYTES {
        return Err(ManifestError::OversizeManifest);
    }
    let raw: RawNativeHostManifest =
        serde_json::from_slice(payload).map_err(|_| ManifestError::InvalidJson)?;
    if raw.manifest_type != "stdio" {
        return Err(ManifestError::UnsupportedManifestType);
    }
    if raw.sniptale_native_host_manifest_version != NATIVE_HOST_MANIFEST_VERSION {
        return Err(ManifestError::ManifestVersionDrift);
    }
    let manifest = NativeHostManifest {
        host_name: raw.name,
        manifest_version: raw.sniptale_native_host_manifest_version,
        allowed_origins: raw.allowed_origins,
        path: raw.path,
        channel: expected_channel,
    };
    validate_host_name(&manifest.host_name)?;
    validate_host_name_matches_channel(&manifest.host_name, manifest.channel)?;
    validate_allowed_origin_shape(&manifest.allowed_origins)?;
    Ok(manifest)
}

pub(crate) fn validate_manifest(
    manifest: &NativeHostManifest,
    expected_channel: InstallChannel,
    expected_executable_path: &str,
    expected_allowed_origins: &[&str],
) -> Result<(), ManifestError> {
    validate_host_name(&manifest.host_name)?;
    validate_host_name_matches_channel(&manifest.host_name, manifest.channel)?;
    validate_manifest_version(&manifest.manifest_version)?;
    validate_channel(manifest.channel, expected_channel)?;
    validate_allowed_origins(&manifest.allowed_origins, expected_allowed_origins)?;
    validate_manifest_path(&manifest.path, expected_executable_path)
}

fn validate_host_name_matches_channel(
    host_name: &str,
    channel: InstallChannel,
) -> Result<(), ManifestError> {
    if host_name == expected_host_name(channel) {
        Ok(())
    } else {
        Err(ManifestError::InvalidHostName)
    }
}

fn validate_host_name(host_name: &str) -> Result<(), ManifestError> {
    if host_name.is_empty()
        || host_name.starts_with('.')
        || host_name.ends_with('.')
        || host_name.contains("..")
        || !host_name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'.')
        })
    {
        Err(ManifestError::InvalidHostName)
    } else {
        Ok(())
    }
}

fn validate_channel(
    channel: InstallChannel,
    expected_channel: InstallChannel,
) -> Result<(), ManifestError> {
    if channel == expected_channel {
        Ok(())
    } else {
        Err(ManifestError::WrongChannel)
    }
}

fn validate_manifest_version(version: &str) -> Result<(), ManifestError> {
    if version == NATIVE_HOST_MANIFEST_VERSION {
        Ok(())
    } else {
        Err(ManifestError::ManifestVersionDrift)
    }
}

fn validate_allowed_origin_shape(allowed_origins: &[String]) -> Result<(), ManifestError> {
    if allowed_origins.is_empty() {
        return Err(ManifestError::MissingAllowedOrigin);
    }
    for origin in allowed_origins {
        if !is_explicit_extension_origin(origin) {
            return Err(ManifestError::InvalidAllowedOrigin);
        }
    }
    Ok(())
}

fn validate_allowed_origins(
    allowed_origins: &[String],
    expected_allowed_origins: &[&str],
) -> Result<(), ManifestError> {
    if allowed_origins.is_empty() || expected_allowed_origins.is_empty() {
        return Err(ManifestError::MissingAllowedOrigin);
    }
    validate_allowed_origin_shape(allowed_origins)?;
    if allowed_origins.len() != expected_allowed_origins.len() {
        return Err(ManifestError::UnexpectedAllowedOrigin);
    }
    for origin in allowed_origins {
        if !expected_allowed_origins
            .iter()
            .any(|expected| origin == expected)
        {
            return Err(ManifestError::UnexpectedAllowedOrigin);
        }
    }
    Ok(())
}

fn is_explicit_extension_origin(origin: &str) -> bool {
    let Some(extension_id) = origin
        .strip_prefix("chrome-extension://")
        .and_then(|rest| rest.strip_suffix('/'))
    else {
        return false;
    };
    extension_id.len() == 32
        && extension_id
            .bytes()
            .all(|byte| (b'a'..=b'p').contains(&byte))
}

fn validate_manifest_path(path: &str, expected_path: &str) -> Result<(), ManifestError> {
    if path == expected_path || path == expected_executable_file_name(expected_path) {
        Ok(())
    } else {
        Err(ManifestError::PathDrift)
    }
}

fn expected_executable_file_name(expected_path: &str) -> &str {
    expected_path
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(expected_path)
}

fn read_error_to_manifest_error(error: &io::Error) -> ManifestError {
    if error.kind() == io::ErrorKind::NotFound {
        ManifestError::MissingManifest
    } else {
        ManifestError::UnreadableManifest
    }
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::MissingManifest => "native host manifest is missing",
            Self::UnreadableManifest => "native host manifest is unreadable",
            Self::OversizeManifest => "native host manifest is too large",
            Self::InvalidJson => "native host manifest JSON is invalid",
            Self::UnsupportedManifestType => "native host manifest type is unsupported",
            Self::ManifestVersionDrift => "native host manifest version drift",
            Self::InvalidHostName => "native host manifest host name is invalid",
            Self::InvalidAllowedOrigin => "native host manifest allowed origin is invalid",
            Self::MissingAllowedOrigin => "native host manifest allowed origin is missing",
            Self::UnexpectedAllowedOrigin => "native host manifest allowed origin is unexpected",
            Self::WrongChannel => "native host manifest channel is wrong",
            Self::PathDrift => "native host manifest path drift",
        })
    }
}

impl std::error::Error for ManifestError {}
