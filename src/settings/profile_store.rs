//! Crash-safe persistence for the versioned local settings authority.

use std::io::Write;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use super::profile::{
    LocalProfile, LocalProfileError, MAX_LOCAL_PROFILE_BYTES, default_extension_profile,
    parse_local_profile, validate_profile,
};
#[cfg(test)]
use super::{ActiveSettings, LocalProfileDraft, SettingsApplyError};

pub(crate) fn load_local_profile_from_path(
    path: Option<PathBuf>,
) -> Result<Option<LocalProfile>, LocalProfileError> {
    let Some(path) = path else {
        return Ok(None);
    };
    match read_local_profile(&path) {
        Ok(payload) => parse_local_profile(&payload).map(Some),
        Err(LocalProfileReadError::NotFound) => Ok(None),
        Err(LocalProfileReadError::Oversize) => Err(LocalProfileError::Oversize),
        Err(LocalProfileReadError::Io) => Err(LocalProfileError::Io),
    }
}

pub(crate) fn ensure_local_profile_from_path(
    path: Option<PathBuf>,
) -> Result<LocalProfile, LocalProfileError> {
    let Some(path) = path else {
        return Ok(default_extension_profile());
    };
    if let Some(profile) = load_local_profile_from_path(Some(path.clone()))? {
        Ok(profile)
    } else {
        let profile = default_extension_profile();
        write_profile_atomically(&path, &profile)?;
        Ok(profile)
    }
}

/// Persists a complete profile candidate before publishing its matching runtime mode epoch.
///
/// A failed write rolls the epoch back to the prior stable authority. A runtime publish failure
/// after the durable replace marks the target degraded, so no ingress is reopened against a
/// profile that the process has not acknowledged.
#[cfg(test)]
pub(crate) fn apply_local_profile_draft_from_path(
    path: Option<PathBuf>,
    active: &ActiveSettings,
    draft: &LocalProfileDraft,
) -> Result<LocalProfile, SettingsApplyError> {
    let transition = active.prepare_profile_transition(draft)?;
    let candidate = ActiveSettings::candidate_profile(&transition).clone();
    let Some(path) = path else {
        return match active.rollback_profile_transition(&transition) {
            Ok(_) => Err(SettingsApplyError::InvalidProfile(LocalProfileError::Io)),
            Err(error) => Err(error),
        };
    };
    if let Err(error) = write_profile_atomically(&path, &candidate) {
        return match active.rollback_profile_transition(&transition) {
            Ok(_) => Err(SettingsApplyError::InvalidProfile(error)),
            Err(rollback_error) => Err(rollback_error),
        };
    }
    match active.commit_profile_transition(&transition) {
        Ok(_) => Ok(candidate),
        Err(error) => match active.degrade_after_persistence(&transition) {
            Ok(_) | Err(_) => Err(error),
        },
    }
}

#[cfg(any(windows, test))]
pub(crate) fn persist_profile_candidate_from_path(
    path: Option<PathBuf>,
    candidate: &LocalProfile,
) -> Result<(), LocalProfileError> {
    let path = path.ok_or(LocalProfileError::Io)?;
    write_profile_atomically(&path, candidate)
}

fn write_profile_atomically(path: &Path, profile: &LocalProfile) -> Result<(), LocalProfileError> {
    validate_profile(profile)?;
    let parent = path.parent().ok_or(LocalProfileError::Io)?;
    fs::create_dir_all(parent).map_err(|_| LocalProfileError::Io)?;
    let payload = serde_json::to_vec(profile).map_err(|_| LocalProfileError::Io)?;
    if payload.len() > MAX_LOCAL_PROFILE_BYTES {
        return Err(LocalProfileError::Oversize);
    }
    let temporary = path.with_extension("json.pending");
    match fs::remove_file(&temporary) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(LocalProfileError::Io),
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|_| LocalProfileError::Io)?;
    if file.write_all(&payload).is_err() || file.sync_all().is_err() {
        let _ = fs::remove_file(&temporary);
        return Err(LocalProfileError::Io);
    }
    fs::rename(&temporary, path).map_err(|_| {
        let _ = fs::remove_file(&temporary);
        LocalProfileError::Io
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LocalProfileReadError {
    NotFound,
    Oversize,
    Io,
}

fn read_local_profile(path: &Path) -> Result<Vec<u8>, LocalProfileReadError> {
    let file = fs::File::open(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            LocalProfileReadError::NotFound
        } else {
            LocalProfileReadError::Io
        }
    })?;
    let mut reader = file.take((MAX_LOCAL_PROFILE_BYTES + 1) as u64);
    let mut payload = Vec::with_capacity(MAX_LOCAL_PROFILE_BYTES.min(4096));
    reader
        .read_to_end(&mut payload)
        .map_err(|_| LocalProfileReadError::Io)?;
    if payload.len() > MAX_LOCAL_PROFILE_BYTES {
        return Err(LocalProfileReadError::Oversize);
    }
    Ok(payload)
}
