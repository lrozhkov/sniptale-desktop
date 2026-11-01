//! Checks an already published file before a crash-recovered job may release its source.

use super::{AssetJob, AssetPublishError, ManualDestination, PublishReceipt};
use crate::transfer::hashing::Sha256Accumulator;
use std::{fs, io::Read, path::PathBuf};

pub(super) fn existing_receipt(
    job: &AssetJob,
) -> Result<Option<PublishReceipt>, AssetPublishError> {
    let destination = ManualDestination::open(
        job.destination_id().to_owned(),
        PathBuf::from(job.destination_root()),
    )
    .map_err(|_| AssetPublishError::DestinationUnavailable)?;
    let path = destination.root().join(job.destination_leaf());
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(AssetPublishError::DestinationUnavailable),
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || Some(metadata.len()) != job.expected_bytes()
    {
        return Err(AssetPublishError::NameCollision);
    }
    let mut file = fs::File::open(path).map_err(|_| AssetPublishError::DestinationUnavailable)?;
    let mut digest = Sha256Accumulator::new();
    let mut count = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| AssetPublishError::DestinationUnavailable)?;
        if read == 0 {
            break;
        }
        count = count
            .checked_add(read as u64)
            .ok_or(AssetPublishError::IntegrityMismatch)?;
        if Some(count) > job.expected_bytes() {
            return Err(AssetPublishError::IntegrityMismatch);
        }
        digest.update(&buffer[..read]);
    }
    let sha256 = digest.finalize_hex();
    if Some(count) != job.expected_bytes() || Some(sha256.as_str()) != job.expected_sha256() {
        return Err(AssetPublishError::IntegrityMismatch);
    }
    Ok(Some(PublishReceipt {
        bytes: count,
        sha256,
    }))
}
