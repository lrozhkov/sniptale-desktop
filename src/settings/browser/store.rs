//! Bounded private target registry storage. Read paths never repair corrupt state.

use super::{BrowserTargetError, BrowserTargetState, validate};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
const MAX_BYTES: usize = 32 * 1024;

pub(super) fn load(path: &Path) -> Result<BrowserTargetState, BrowserTargetError> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(BrowserTargetState::default());
        }
        Err(_) => return Err(BrowserTargetError::Io),
    };
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| BrowserTargetError::Io)?;
    if bytes.len() > MAX_BYTES {
        return Err(BrowserTargetError::Limit);
    }
    let state = serde_json::from_slice(&bytes).map_err(|_| BrowserTargetError::Invalid)?;
    validate(&state)?;
    Ok(state)
}

pub(super) fn save(path: &Path, state: &BrowserTargetState) -> Result<(), BrowserTargetError> {
    let bytes = serde_json::to_vec(state).map_err(|_| BrowserTargetError::Invalid)?;
    if bytes.len() > MAX_BYTES {
        return Err(BrowserTargetError::Limit);
    }
    fs::create_dir_all(path.parent().ok_or(BrowserTargetError::Io)?)
        .map_err(|_| BrowserTargetError::Io)?;
    let pending = path.with_extension("pending");
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&pending)
        .map_err(|_| BrowserTargetError::Io)?;
    let result = (|| {
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| BrowserTargetError::Io)?;
        drop(file);
        fs::rename(&pending, path).map_err(|_| BrowserTargetError::Io)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&pending);
    }
    result
}
