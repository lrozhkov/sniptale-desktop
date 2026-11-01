//! Explicit Desktop exit prevents browser retries from starting a new resident process.

use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BrowserStartupError {
    Io,
    Invalid,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StartupPreference {
    schema_version: u16,
    browser_start_allowed: bool,
}

pub(crate) fn browser_start_allowed(path: &Path) -> Result<bool, BrowserStartupError> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(_) => return Err(BrowserStartupError::Io),
    };
    let mut bytes = Vec::new();
    file.take(513)
        .read_to_end(&mut bytes)
        .map_err(|_| BrowserStartupError::Io)?;
    if bytes.len() > 512 {
        return Err(BrowserStartupError::Invalid);
    }
    let preference: StartupPreference =
        serde_json::from_slice(&bytes).map_err(|_| BrowserStartupError::Invalid)?;
    if preference.schema_version != 1 {
        return Err(BrowserStartupError::Invalid);
    }
    Ok(preference.browser_start_allowed)
}

pub(crate) fn persist_browser_start_allowed(
    path: &Path,
    allowed: bool,
) -> Result<(), BrowserStartupError> {
    fs::create_dir_all(path.parent().ok_or(BrowserStartupError::Io)?)
        .map_err(|_| BrowserStartupError::Io)?;
    let bytes = serde_json::to_vec(&StartupPreference {
        schema_version: 1,
        browser_start_allowed: allowed,
    })
    .map_err(|_| BrowserStartupError::Invalid)?;
    let pending = path.with_extension("pending");
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&pending)
        .map_err(|_| BrowserStartupError::Io)?;
    if file
        .write_all(&bytes)
        .and_then(|()| file.sync_all())
        .and_then(|()| fs::rename(&pending, path))
        .is_err()
    {
        let _ = fs::remove_file(pending);
        return Err(BrowserStartupError::Io);
    }
    Ok(())
}

#[cfg(all(windows, not(test)))]
pub(crate) fn browser_startup_preference_path() -> Option<std::path::PathBuf> {
    Some(
        std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
            .join("Sniptale")
            .join("NativeAgent")
            .join("browser-startup.json"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_exit_survives_restart_and_corrupt_preference_fails_closed()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("startup.json");
        assert_eq!(browser_start_allowed(&path), Ok(true));
        persist_browser_start_allowed(&path, false).map_err(|_| "save")?;
        assert_eq!(browser_start_allowed(&path), Ok(false));
        persist_browser_start_allowed(&path, true).map_err(|_| "save")?;
        assert_eq!(browser_start_allowed(&path), Ok(true));
        for bytes in [
            b"invalid".to_vec(),
            vec![b' '; 513],
            br#"{"schemaVersion":2,"browserStartAllowed":true}"#.to_vec(),
        ] {
            fs::write(&path, bytes)?;
            assert!(browser_start_allowed(&path).is_err());
        }
        Ok(())
    }
}
