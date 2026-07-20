use std::{fs, path::PathBuf};

use tempfile::tempdir;

use super::{
    AssetFormat, AssetJob, AssetJobId, AssetJobState, AssetJobStore, AssetPublishError,
    AssetPublishService, AssetPublisher, FinalizedArtifactSource, FinalizedArtifactSourceError,
    ManualDestination, prepare_manual_asset_job,
};
use crate::transfer::hashing::sha256_hex;
use crate::transfer::{
    ProtectedMediaReader,
    protected_file::{PROTECTED_MEDIA_ALGORITHM, protect_file_in_place},
    protected_key::{new_protected_media_key, unprotect_media_key},
};

mod recovery;

struct BytesSource {
    bytes: Vec<u8>,
    sha256: String,
    offset: usize,
    fail: bool,
    tamper: bool,
}

impl BytesSource {
    fn new(bytes: Vec<u8>) -> Self {
        Self {
            sha256: sha256_hex(&bytes),
            bytes,
            offset: 0,
            fail: false,
            tamper: false,
        }
    }

    fn failing(bytes: Vec<u8>) -> Self {
        Self {
            sha256: sha256_hex(&bytes),
            bytes,
            offset: 0,
            fail: true,
            tamper: false,
        }
    }

    fn tampered(bytes: Vec<u8>) -> Self {
        Self {
            sha256: sha256_hex(&bytes),
            bytes,
            offset: 0,
            fail: false,
            tamper: true,
        }
    }
}

impl FinalizedArtifactSource for BytesSource {
    fn total_bytes(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn sha256(&self) -> &str {
        &self.sha256
    }

    fn read_next(
        &mut self,
        maximum_bytes: usize,
    ) -> Result<Option<Vec<u8>>, FinalizedArtifactSourceError> {
        if self.fail && self.offset > 0 {
            return Err(FinalizedArtifactSourceError::Unavailable);
        }
        if self.tamper && self.offset > 0 {
            return Err(FinalizedArtifactSourceError::Tampered);
        }
        if self.offset == self.bytes.len() {
            return Ok(None);
        }
        let end = (self.offset + maximum_bytes.min(3)).min(self.bytes.len());
        let chunk = self.bytes[self.offset..end].to_vec();
        self.offset = end;
        Ok(Some(chunk))
    }
}

fn destination(root: PathBuf) -> Result<ManualDestination, super::ManualDestinationError> {
    ManualDestination::open("destination-1".to_owned(), root)
}

fn png() -> Vec<u8> {
    b"\x89PNG\r\n\x1a\npixels".to_vec()
}

fn prepared_job(root: PathBuf, bytes: &[u8]) -> Result<AssetJob, super::AssetJobError> {
    let mut job = AssetJob::prepare(
        AssetJobId::parse("asset-job-1".to_owned())
            .map_err(|_| super::AssetJobError::InvalidMetadata)?,
        AssetFormat::Png,
        &destination(root).map_err(|_| super::AssetJobError::InvalidMetadata)?,
        "sniptale-screenshot-1.png".to_owned(),
    )?;
    job.bind_artifact(bytes.len() as u64, sha256_hex(bytes))?;
    Ok(job)
}

#[test]
fn publisher_writes_only_verified_png_and_emits_a_receipt() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = tempdir()?;
    let bytes = png();
    let mut job = prepared_job(directory.path().to_path_buf(), &bytes)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let mut source = BytesSource::new(bytes.clone());

    let receipt = AssetPublisher::publish(&mut job, &mut source)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;

    assert_eq!(receipt.bytes, bytes.len() as u64);
    assert_eq!(receipt.sha256, sha256_hex(&bytes));
    assert_eq!(job.state(), AssetJobState::Published);
    assert_eq!(
        fs::read(directory.path().join("sniptale-screenshot-1.png"))?,
        bytes
    );
    assert!(
        !directory
            .path()
            .join(".sniptale-asset-job-1.partial")
            .exists()
    );
    Ok(())
}

#[test]
fn publisher_never_overwrites_a_user_file_or_leaves_a_partial_after_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let bytes = png();
    let target = directory.path().join("sniptale-screenshot-1.png");
    fs::write(&target, b"user-file")?;
    let mut collision = prepared_job(directory.path().to_path_buf(), &bytes)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let mut source = BytesSource::new(bytes.clone());

    assert_eq!(
        AssetPublisher::publish(&mut collision, &mut source),
        Err(AssetPublishError::NameCollision)
    );
    assert_eq!(collision.state(), AssetJobState::Failed);
    assert_eq!(fs::read(&target)?, b"user-file");

    fs::remove_file(&target)?;
    let mut failed = prepared_job(directory.path().to_path_buf(), &bytes)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let mut interrupted = BytesSource::failing(bytes);
    assert_eq!(
        AssetPublisher::publish(&mut failed, &mut interrupted),
        Err(AssetPublishError::SourceUnavailable)
    );
    assert_eq!(failed.state(), AssetJobState::Failed);
    assert!(
        !directory
            .path()
            .join(".sniptale-asset-job-1.partial")
            .exists()
    );
    let mut tampered = BytesSource::tampered(png());
    assert_eq!(
        AssetPublisher::publish(&mut failed, &mut tampered),
        Err(AssetPublishError::SourceTampered)
    );
    assert!(
        !directory
            .path()
            .join(".sniptale-asset-job-1.partial")
            .exists()
    );
    Ok(())
}

#[test]
fn job_store_rejects_tampering_and_recovers_exact_private_job()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let bytes = png();
    let job = prepared_job(directory.path().to_path_buf(), &bytes)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let store = AssetJobStore::open(directory.path().join("jobs"))
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;

    store
        .save(&job)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    assert_eq!(
        store
            .load(job.id())
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?,
        Some(job.clone())
    );

    fs::write(
        directory.path().join("jobs").join("asset-job-1.json"),
        b"not-json",
    )?;
    assert_eq!(
        store.load(job.id()),
        Err(super::AssetJobStoreError::InvalidJob)
    );
    Ok(())
}

#[test]
fn prepared_job_is_durable_before_media_and_requires_explicit_artifact_binding()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let bytes = png();
    let destination = destination(directory.path().to_path_buf())
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let mut job = AssetJob::prepare(
        AssetJobId::parse("asset-job-prepared".to_owned())
            .map_err(|_| std::io::Error::other("id"))?,
        AssetFormat::Png,
        &destination,
        "sniptale-prepared.png".to_owned(),
    )
    .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let store = AssetJobStore::open(directory.path().join("jobs"))
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    store
        .save(&job)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    assert_eq!(
        store
            .load(job.id())
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?,
        Some(job.clone())
    );
    assert_eq!(job.state(), AssetJobState::Prepared);

    let mut source = BytesSource::new(bytes.clone());
    assert_eq!(
        AssetPublisher::publish(&mut job, &mut source),
        Err(AssetPublishError::InvalidJob)
    );
    job.bind_artifact(bytes.len() as u64, sha256_hex(&bytes))
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    assert_eq!(job.state(), AssetJobState::Pending);
    Ok(())
}

#[test]
fn route_binding_creates_a_safe_durable_job_before_media_is_available()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let job = prepare_manual_asset_job(
        format!("asset-{}", "a".repeat(64)),
        AssetFormat::Png,
        "destination-1".to_owned(),
        directory.path().to_path_buf(),
    )
    .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    assert_eq!(job.state(), AssetJobState::Prepared);
    assert_eq!(
        job.destination_leaf(),
        format!("asset-{}.png", "a".repeat(64))
    );
    Ok(())
}

#[test]
fn service_binds_a_prepared_job_before_rehydrating_its_finalized_artifact()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let bytes = png();
    let job = prepare_manual_asset_job(
        "asset-job-1".to_owned(),
        AssetFormat::Png,
        "destination-1".to_owned(),
        directory.path().to_path_buf(),
    )
    .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let service = AssetPublishService::open(directory.path().join("jobs"))
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    service
        .save_prepared(&job)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let id = job.id().clone();
    let bound = service
        .bind_prepared(&id)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    assert_eq!(bound.state(), AssetJobState::BoundAwaitingArtifact);
    let mut source = BytesSource::new(bytes.clone());
    service
        .publish_finalized(&id, &mut source)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    assert_eq!(fs::read(directory.path().join("asset-job-1.png"))?, bytes);
    Ok(())
}

#[test]
fn publisher_consumes_the_transfer_owned_protected_reader_port()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let private_media = directory.path().join("private.png");
    let bytes = png();
    fs::write(&private_media, &bytes)?;
    let (protection, key) = new_protected_media_key(PROTECTED_MEDIA_ALGORITHM)
        .map_err(|_| std::io::Error::other("key"))?;
    let (_, nonce_prefix) = unprotect_media_key(&protection, PROTECTED_MEDIA_ALGORITHM)
        .map_err(|_| std::io::Error::other("key"))?;
    protect_file_in_place(&private_media, &key, nonce_prefix)
        .map_err(|_| std::io::Error::other("protection"))?;
    let mut reader = ProtectedMediaReader::open(
        private_media,
        &protection,
        bytes.len() as u64,
        sha256_hex(&bytes),
    )
    .map_err(|_| std::io::Error::other("reader"))?;
    let output = directory.path().join("output");
    fs::create_dir(&output)?;
    let mut job = prepared_job(output.clone(), &bytes)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;

    AssetPublisher::publish(&mut job, &mut reader)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;

    assert_eq!(fs::read(output.join("sniptale-screenshot-1.png"))?, bytes);
    assert_eq!(job.state(), AssetJobState::Published);
    Ok(())
}
