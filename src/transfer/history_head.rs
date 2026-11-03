//! Bounded authenticated commit heads shared by independently owned history formats.

use super::{
    trajectory_crypto::{TrajectoryAuthentication, TrajectoryCipher},
    trajectory_ledger::{TrajectoryStoreError, check_private_path},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

const MAX_HEAD_BYTES: u64 = 16384;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Envelope<T> {
    ledger: T,
    authentication: Vec<u8>,
}

pub(super) fn read<T: DeserializeOwned + Serialize>(
    path: &Path,
    open: impl FnOnce(&T) -> Result<TrajectoryCipher, TrajectoryStoreError>,
) -> Result<(T, TrajectoryCipher), TrajectoryStoreError> {
    check_private_path(path, false)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|_| TrajectoryStoreError::Io)?
        .take(MAX_HEAD_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| TrajectoryStoreError::Io)?;
    if bytes.len() as u64 > MAX_HEAD_BYTES {
        return Err(TrajectoryStoreError::Invalid);
    }
    let envelope: Envelope<T> =
        serde_json::from_slice(&bytes).map_err(|_| TrajectoryStoreError::Invalid)?;
    let cipher = open(&envelope.ledger)?;
    let payload =
        serde_json::to_vec(&envelope.ledger).map_err(|_| TrajectoryStoreError::Invalid)?;
    cipher
        .verify(
            TrajectoryAuthentication::Ledger,
            &payload,
            &envelope.authentication,
        )
        .map_err(|_| TrajectoryStoreError::Authentication)?;
    Ok((envelope.ledger, cipher))
}

/// The caller validates its head and holds its exclusive stream lock through rename.
pub(super) fn write<T: Serialize>(
    value: &T,
    path: &Path,
    cipher: &TrajectoryCipher,
) -> Result<(), TrajectoryStoreError> {
    let payload = serde_json::to_vec(value).map_err(|_| TrajectoryStoreError::Invalid)?;
    let authentication = cipher
        .authenticate(TrajectoryAuthentication::Ledger, &payload)
        .map_err(|_| TrajectoryStoreError::Authentication)?
        .to_vec();
    let bytes = serde_json::to_vec(&Envelope {
        ledger: value,
        authentication,
    })
    .map_err(|_| TrajectoryStoreError::Invalid)?;
    if bytes.len() as u64 > MAX_HEAD_BYTES {
        return Err(TrajectoryStoreError::Invalid);
    }
    let pending = path.with_extension("pending");
    check_private_path(&pending, true)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&pending)
        .map_err(|_| TrajectoryStoreError::Io)?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| TrajectoryStoreError::Io)?;
    drop(file);
    fs::rename(&pending, path).map_err(|_| TrajectoryStoreError::Io)?;
    #[cfg(not(windows))]
    fs::File::open(path.parent().ok_or(TrajectoryStoreError::Invalid)?)
        .and_then(|directory| directory.sync_all())
        .map_err(|_| TrajectoryStoreError::Io)?;
    Ok(())
}
