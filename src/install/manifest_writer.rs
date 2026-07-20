//! Native Messaging manifest template rendering.

use std::{error::Error, fmt};

use serde::Serialize;

use super::registry::{
    InstallChannel, ManifestError, NATIVE_HOST_MANIFEST_VERSION, NativeHostManifest,
    expected_host_name, validate_manifest,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativeHostManifestTemplate<'a> {
    pub(crate) channel: InstallChannel,
    pub(crate) executable_path: &'a str,
    pub(crate) allowed_origins: &'a [&'a str],
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ManifestTemplateError {
    EmptyExecutablePath,
    InvalidManifest(ManifestError),
    Json,
}

impl fmt::Display for ManifestTemplateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyExecutablePath => formatter.write_str("manifest executable path is empty"),
            Self::InvalidManifest(error) => {
                write!(formatter, "manifest template is invalid: {error}")
            }
            Self::Json => formatter.write_str("manifest template JSON serialization failed"),
        }
    }
}

impl Error for ManifestTemplateError {}

#[derive(Serialize)]
struct RenderedNativeHostManifest<'a> {
    name: &'static str,
    description: &'static str,
    path: &'a str,
    #[serde(rename = "type")]
    manifest_type: &'static str,
    sniptale_native_host_manifest_version: &'static str,
    allowed_origins: Vec<&'a str>,
}

pub(crate) fn render_native_host_manifest(
    template: NativeHostManifestTemplate<'_>,
) -> Result<String, ManifestTemplateError> {
    if template.executable_path.trim().is_empty() {
        return Err(ManifestTemplateError::EmptyExecutablePath);
    }
    let host_name = expected_host_name(template.channel);
    let manifest = NativeHostManifest {
        host_name: host_name.to_owned(),
        manifest_version: NATIVE_HOST_MANIFEST_VERSION.to_owned(),
        allowed_origins: template
            .allowed_origins
            .iter()
            .map(|origin| (*origin).to_owned())
            .collect(),
        path: template.executable_path.to_owned(),
        channel: template.channel,
    };
    validate_manifest(
        &manifest,
        template.channel,
        template.executable_path,
        template.allowed_origins,
    )
    .map_err(ManifestTemplateError::InvalidManifest)?;

    serde_json::to_string_pretty(&RenderedNativeHostManifest {
        name: host_name,
        description: "Sniptale Native Host",
        path: template.executable_path,
        manifest_type: "stdio",
        sniptale_native_host_manifest_version: NATIVE_HOST_MANIFEST_VERSION,
        allowed_origins: template.allowed_origins.to_vec(),
    })
    .map_err(|_| ManifestTemplateError::Json)
}
