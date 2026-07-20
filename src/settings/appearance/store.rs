//! Crash-safe filesystem adapter for the native appearance preference.

use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
};

use super::{MAX_UI_PREFERENCES_BYTES, UiPreferences, UiPreferencesError, parse};

pub(super) fn read(path: Option<PathBuf>) -> Result<Option<UiPreferences>, UiPreferencesError> {
    let path = path.ok_or(UiPreferencesError::Io)?;
    let file = match OpenOptions::new().read(true).open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(UiPreferencesError::Io),
    };
    let length = file.metadata().map_err(|_| UiPreferencesError::Io)?.len();
    if length > MAX_UI_PREFERENCES_BYTES as u64 {
        return Err(UiPreferencesError::Oversize);
    }
    let mut payload = Vec::with_capacity(usize::try_from(length).unwrap_or(0));
    file.take((MAX_UI_PREFERENCES_BYTES + 1) as u64)
        .read_to_end(&mut payload)
        .map_err(|_| UiPreferencesError::Io)?;
    parse(&payload).map(Some)
}

pub(super) fn write(
    path: Option<PathBuf>,
    preferences: UiPreferences,
) -> Result<(), UiPreferencesError> {
    let path = path.ok_or(UiPreferencesError::Io)?;
    write_path(&path, preferences, || Ok(()))
}

fn write_path(
    path: &std::path::Path,
    preferences: UiPreferences,
    before_replace: impl FnOnce() -> Result<(), UiPreferencesError>,
) -> Result<(), UiPreferencesError> {
    let parent = path.parent().ok_or(UiPreferencesError::Io)?;
    fs::create_dir_all(parent).map_err(|_| UiPreferencesError::Io)?;
    let payload = serde_json::to_vec(&preferences).map_err(|_| UiPreferencesError::Io)?;
    if payload.len() > MAX_UI_PREFERENCES_BYTES {
        return Err(UiPreferencesError::Oversize);
    }
    let temporary = path.with_extension("json.tmp");
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)
        .map_err(|_| UiPreferencesError::Io)?;
    if file
        .write_all(&payload)
        .and_then(|()| file.sync_all())
        .is_err()
    {
        drop(file);
        let _ = fs::remove_file(&temporary);
        return Err(UiPreferencesError::Io);
    }
    drop(file);
    if let Err(error) = before_replace() {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    fs::rename(&temporary, path).map_err(|_| {
        let _ = fs::remove_file(&temporary);
        UiPreferencesError::Io
    })
}

#[cfg(test)]
pub(super) fn write_with_failed_replace(
    path: &std::path::Path,
    preferences: UiPreferences,
) -> Result<(), UiPreferencesError> {
    write_path(path, preferences, || Err(UiPreferencesError::Io))
}
