//! Bounded, no-overwrite plaintext publication into a validated Manual destination.

use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};

use crate::transfer::hashing::Sha256Accumulator;

use super::{
    AssetJob, AssetJobError, FinalizedArtifactSource, FinalizedArtifactSourceError,
    ManualDestination,
};

const MAX_PUBLISH_CHUNK_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PublishReceipt {
    pub(crate) bytes: u64,
    pub(crate) sha256: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AssetPublishError {
    InvalidJob,
    #[cfg(test)]
    SourceUnavailable,
    SourceTampered,
    InvalidChunk,
    IntegrityMismatch,
    InvalidFormat,
    DestinationUnavailable,
    NameCollision,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AssetPublisher;

impl AssetPublisher {
    pub(crate) fn publish(
        job: &mut AssetJob,
        source: &mut dyn FinalizedArtifactSource,
    ) -> Result<PublishReceipt, AssetPublishError> {
        job.validate().map_err(|_| AssetPublishError::InvalidJob)?;
        job.begin_publish().map_err(map_job_error)?;
        let result = Self::publish_started(job, source);
        if result.is_err() {
            job.mark_failed();
        }
        result
    }

    fn publish_started(
        job: &mut AssetJob,
        source: &mut dyn FinalizedArtifactSource,
    ) -> Result<PublishReceipt, AssetPublishError> {
        let expected_bytes = job.expected_bytes().ok_or(AssetPublishError::InvalidJob)?;
        let expected_sha256 = job.expected_sha256().ok_or(AssetPublishError::InvalidJob)?;
        if source.total_bytes() != expected_bytes || source.sha256() != expected_sha256 {
            return Err(AssetPublishError::IntegrityMismatch);
        }
        let destination = ManualDestination::open(
            job.destination_id().to_owned(),
            PathBuf::from(job.destination_root()),
        )
        .map_err(|_| AssetPublishError::DestinationUnavailable)?;
        let root = destination.root();
        let partial = root.join(format!(".sniptale-{}.partial", job.id().as_str()));
        let target = root.join(job.destination_leaf());
        let mut partial_file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)
            .map_err(|_| AssetPublishError::NameCollision)?;
        let write_result = write_source(&mut partial_file, expected_bytes, expected_sha256, source);
        if write_result.is_err() || partial_file.sync_all().is_err() {
            let _ = fs::remove_file(&partial);
            return write_result.and(Err(AssetPublishError::DestinationUnavailable));
        }
        let receipt = write_result?;
        if !matches_format(job.format(), &partial) {
            let _ = fs::remove_file(&partial);
            return Err(AssetPublishError::InvalidFormat);
        }
        if fs::hard_link(&partial, &target).is_err() {
            let _ = fs::remove_file(&partial);
            return Err(AssetPublishError::NameCollision);
        }
        fs::remove_file(&partial).map_err(|_| AssetPublishError::DestinationUnavailable)?;
        job.mark_published().map_err(map_job_error)?;
        Ok(receipt)
    }
}

fn write_source(
    destination: &mut fs::File,
    expected_bytes: u64,
    expected_sha256: &str,
    source: &mut dyn FinalizedArtifactSource,
) -> Result<PublishReceipt, AssetPublishError> {
    let mut bytes = 0_u64;
    let mut hash = Sha256Accumulator::new();
    while let Some(chunk) = source
        .read_next(MAX_PUBLISH_CHUNK_BYTES)
        .map_err(map_source_error)?
    {
        if chunk.is_empty() || chunk.len() > MAX_PUBLISH_CHUNK_BYTES {
            return Err(AssetPublishError::InvalidChunk);
        }
        bytes = bytes
            .checked_add(u64::try_from(chunk.len()).map_err(|_| AssetPublishError::InvalidChunk)?)
            .ok_or(AssetPublishError::InvalidChunk)?;
        if bytes > expected_bytes || destination.write_all(&chunk).is_err() {
            return Err(AssetPublishError::DestinationUnavailable);
        }
        hash.update(&chunk);
    }
    let sha256 = hash.finalize_hex();
    if bytes != expected_bytes || sha256 != expected_sha256 {
        return Err(AssetPublishError::IntegrityMismatch);
    }
    Ok(PublishReceipt { bytes, sha256 })
}

fn matches_format(format: super::AssetFormat, path: &std::path::Path) -> bool {
    let Ok(bytes) = fs::read(path) else {
        return false;
    };
    match format {
        super::AssetFormat::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        super::AssetFormat::Mp4 => bytes.len() >= 12 && &bytes[4..8] == b"ftyp",
    }
}

const fn map_job_error(_error: AssetJobError) -> AssetPublishError {
    AssetPublishError::InvalidJob
}

const fn map_source_error(error: FinalizedArtifactSourceError) -> AssetPublishError {
    match error {
        #[cfg(test)]
        FinalizedArtifactSourceError::Unavailable => AssetPublishError::SourceUnavailable,
        FinalizedArtifactSourceError::Tampered => AssetPublishError::SourceTampered,
    }
}
