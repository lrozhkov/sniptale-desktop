//! Versioned Manual-mode profile parsing and health classification.

#[cfg(any(windows, test))]
use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[cfg(any(windows, test))]
use super::manual::{ManualDestinationSnapshot, ManualRecordingSettings};
use super::manual::{ManualProfile, validate_manual_profile};

const LOCAL_PROFILE_FILE_NAME: &str = "local-profile.json";
pub(super) const NATIVE_LOCAL_PROFILE_SCHEMA_VERSION: u16 = 1;
pub(super) const MAX_LOCAL_PROFILE_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LocalProfileHealth {
    Missing,
    Available(OperatingMode),
    RepairRequired(LocalProfileError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LocalProfileError {
    Io,
    Oversize,
    InvalidJson,
    UnsupportedSchema,
    InvalidProfile,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum OperatingMode {
    ExtensionManaged,
    Manual,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LocalProfile {
    pub(super) schema_version: u16,
    pub(super) revision: u64,
    pub(super) operating_mode: OperatingMode,
    pub(super) manual_profile: Option<ManualProfile>,
}

impl LocalProfile {
    #[must_use]
    pub(crate) const fn operating_mode(&self) -> OperatingMode {
        self.operating_mode
    }

    #[cfg(test)]
    pub(crate) fn with_operating_mode_for_test(mut self, operating_mode: OperatingMode) -> Self {
        self.operating_mode = operating_mode;
        self
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) fn manual_destination(&self) -> Option<ManualDestinationSnapshot> {
        self.manual_profile
            .as_ref()
            .map(ManualProfile::destination_snapshot)
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) fn manual_recording_settings(&self) -> Option<ManualRecordingSettings> {
        self.manual_profile
            .as_ref()
            .map(ManualProfile::recording_settings)
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) fn manual_hotkeys(&self) -> Option<BTreeMap<String, String>> {
        self.manual_profile
            .as_ref()
            .map(|manual| manual.hotkeys.clone())
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) fn manual_screenshot_include_cursor(&self) -> Option<bool> {
        self.manual_profile
            .as_ref()
            .map(|manual| manual.screenshot.include_cursor)
    }
}

pub(super) fn default_extension_profile() -> LocalProfile {
    LocalProfile {
        schema_version: NATIVE_LOCAL_PROFILE_SCHEMA_VERSION,
        revision: 1,
        operating_mode: OperatingMode::ExtensionManaged,
        manual_profile: None,
    }
}

pub(crate) fn local_profile_health_from_path(path: Option<PathBuf>) -> LocalProfileHealth {
    match super::profile_store::load_local_profile_from_path(path) {
        Ok(Some(profile)) => LocalProfileHealth::Available(profile.operating_mode()),
        Ok(None) => LocalProfileHealth::Missing,
        Err(error) => LocalProfileHealth::RepairRequired(error),
    }
}

/// Whether a browser Native Messaging process may begin resident-pipe admission.
///
/// Missing state preserves the existing `ExtensionManaged` default. A Manual profile or an invalid
/// profile fails closed before pipe credentials are read or a resident agent is spawned.
#[must_use]
pub(crate) fn extension_admission_allowed_from_path(path: Option<PathBuf>) -> bool {
    matches!(
        local_profile_health_from_path(path),
        LocalProfileHealth::Missing
            | LocalProfileHealth::Available(OperatingMode::ExtensionManaged)
    )
}

pub(super) fn parse_local_profile(payload: &[u8]) -> Result<LocalProfile, LocalProfileError> {
    if payload.len() > MAX_LOCAL_PROFILE_BYTES {
        return Err(LocalProfileError::Oversize);
    }
    let profile = serde_json::from_slice::<LocalProfile>(payload)
        .map_err(|_| LocalProfileError::InvalidJson)?;
    validate_profile(&profile)?;
    Ok(profile)
}

pub(super) fn validate_profile(profile: &LocalProfile) -> Result<(), LocalProfileError> {
    if profile.schema_version != NATIVE_LOCAL_PROFILE_SCHEMA_VERSION {
        return Err(LocalProfileError::UnsupportedSchema);
    }
    if profile.revision == 0 {
        return Err(LocalProfileError::InvalidProfile);
    }
    if profile.operating_mode == OperatingMode::Manual && profile.manual_profile.is_none() {
        return Err(LocalProfileError::InvalidProfile);
    }
    if let Some(manual) = &profile.manual_profile {
        validate_manual_profile(manual)?;
    }
    Ok(())
}

#[must_use]
pub(crate) const fn local_profile_file_name() -> &'static str {
    LOCAL_PROFILE_FILE_NAME
}

#[cfg(test)]
mod tests;
#[cfg(test)]
#[path = "profile/transactions.rs"]
mod transactions;
