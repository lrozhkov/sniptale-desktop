//! Owner-private history identity paths and bounded file-lock lifecycle.

use super::{
    hashing::sha256_hex,
    trajectory_ledger::{TrajectoryStoreError, check_private_path},
};
use crate::protocol::trajectory::valid_trajectory_id;
use std::{
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};

pub(super) fn archive_path(
    root: &Path,
    recording: &str,
    trajectory: &str,
) -> Result<PathBuf, TrajectoryStoreError> {
    if recording.is_empty()
        || recording.len() > 128
        || !valid_trajectory_id(trajectory)
        || !recording
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
    {
        return Err(TrajectoryStoreError::Invalid);
    }
    Ok(root.join(format!("{}-{trajectory}", sha256_hex(recording.as_bytes()))))
}

pub(super) fn reject_link(path: &Path) -> Result<(), TrajectoryStoreError> {
    check_private_path(path, false)
}

pub(super) fn acquire_lock(
    directory: &Path,
    create: bool,
    writable: bool,
) -> Result<File, TrajectoryStoreError> {
    let path = directory.join("writer.lock");
    check_private_path(&path, create)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(writable)
        .create(create)
        .truncate(false)
        .open(path)
        .map_err(|_| TrajectoryStoreError::Io)?;
    if writable {
        lock.try_lock().map_err(|_| TrajectoryStoreError::Busy)?;
    } else {
        lock.try_lock_shared()
            .map_err(|_| TrajectoryStoreError::Busy)?;
    }
    Ok(lock)
}

pub(super) fn new_file(path: &Path) -> Result<File, TrajectoryStoreError> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| TrajectoryStoreError::Io)
}

pub(super) fn fresh_generation() -> Result<String, TrajectoryStoreError> {
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| TrajectoryStoreError::Authentication)?;
    Ok(sha256_hex(&bytes)[..32].to_owned())
}

const ERASURE_REQUEST: &str = "erase.requested";

/// A durable explicit request fences readers even if retiring the authenticated head fails.
pub(super) fn request_erasure(directory: &Path) -> Result<(), TrajectoryStoreError> {
    let path = directory.join(ERASURE_REQUEST);
    check_private_path(&path, true)?;
    let marker = match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(marker) => marker,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if !erasure_requested(directory)? {
                return Err(TrajectoryStoreError::Invalid);
            }
            OpenOptions::new()
                .write(true)
                .open(&path)
                .map_err(|_| TrajectoryStoreError::Io)?
        }
        Err(_) => return Err(TrajectoryStoreError::Io),
    };
    marker.sync_all().map_err(|_| TrajectoryStoreError::Io)?;
    #[cfg(not(windows))]
    File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(|_| TrajectoryStoreError::Io)?;
    Ok(())
}

pub(super) fn erasure_requested(directory: &Path) -> Result<bool, TrajectoryStoreError> {
    let path = directory.join(ERASURE_REQUEST);
    check_private_path(&path, true)?;
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() && metadata.len() == 0 => Ok(true),
        Ok(_) => Err(TrajectoryStoreError::Invalid),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(TrajectoryStoreError::Io),
    }
}

/// Startup retries only persisted explicit erasures; elapsed time never authorizes disposal.
pub(super) fn finish_erasure(directory: &Path) -> Result<(), TrajectoryStoreError> {
    check_private_path(directory, false)?;
    let _lock = acquire_lock(directory, false, true)?;
    if !erasure_requested(directory)? {
        return Err(TrajectoryStoreError::Invalid);
    }
    #[cfg(not(windows))]
    let mut changed = false;
    for name in ["blocks.bin", "index.bin", "head.pending", "head.json"] {
        let path = directory.join(name);
        check_private_path(&path, true)?;
        match std::fs::remove_file(path) {
            Ok(()) => {
                #[cfg(not(windows))]
                {
                    changed = true;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(TrajectoryStoreError::Io),
        }
    }
    #[cfg(not(windows))]
    if changed {
        File::open(directory)
            .and_then(|file| file.sync_all())
            .map_err(|_| TrajectoryStoreError::Io)?;
    }
    Ok(())
}
