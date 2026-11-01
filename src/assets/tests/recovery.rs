use tempfile::tempdir;

use super::{
    AssetFormat, AssetJob, AssetJobId, AssetJobStore, AssetPublishError, AssetPublisher,
    BytesSource, FinalizedArtifactSource, FinalizedArtifactSourceError, ManualDestination, png,
    prepare_manual_asset_job, prepared_job,
};
use crate::transfer::hashing::sha256_hex;

struct EmptyChunkSource {
    sha256: String,
    sent: bool,
}

impl FinalizedArtifactSource for EmptyChunkSource {
    fn total_bytes(&self) -> u64 {
        1
    }

    fn sha256(&self) -> &str {
        &self.sha256
    }

    fn read_next(
        &mut self,
        _maximum_bytes: usize,
    ) -> Result<Option<Vec<u8>>, FinalizedArtifactSourceError> {
        if self.sent {
            Ok(None)
        } else {
            self.sent = true;
            Ok(Some(Vec::new()))
        }
    }
}

#[test]
fn recovery_reloads_the_exact_bound_job_without_reconstructing_a_destination() -> Result<(), String>
{
    let directory = tempdir().map_err(|error| error.to_string())?;
    let job = prepared_job(directory.path().to_path_buf(), &png())
        .map_err(|error| format!("prepare: {error:?}"))?;
    let root = directory.path().join("jobs");
    AssetJobStore::open(root.clone())
        .map_err(|error| format!("open: {error:?}"))?
        .save(&job)
        .map_err(|error| format!("save: {error:?}"))?;

    let recovered = AssetJobStore::open(root)
        .map_err(|error| format!("reopen: {error:?}"))?
        .load(job.id())
        .map_err(|error| format!("load: {error:?}"))?;
    assert_eq!(recovered, Some(job));
    Ok(())
}

#[test]
fn recovery_rejects_invalid_destination_and_incomplete_job_transitions() -> Result<(), String> {
    let directory = tempdir().map_err(|error| error.to_string())?;
    assert!(ManualDestination::open(String::new(), directory.path().to_path_buf()).is_err());
    assert!(
        ManualDestination::open("destination".to_owned(), directory.path().join("missing"))
            .is_err()
    );
    assert!(
        prepare_manual_asset_job(
            "invalid/id".to_owned(),
            AssetFormat::Png,
            "destination".to_owned(),
            directory.path().to_path_buf(),
        )
        .is_err()
    );

    let destination =
        ManualDestination::open("destination".to_owned(), directory.path().to_path_buf())
            .map_err(|error| format!("destination: {error:?}"))?;
    let mut job = AssetJob::prepare(
        AssetJobId::parse("asset-job-1".to_owned()).map_err(|error| format!("id: {error:?}"))?,
        AssetFormat::Mp4,
        &destination,
        "recording-1.mp4".to_owned(),
    )
    .map_err(|error| format!("job: {error:?}"))?;
    assert!(job.bind_artifact(0, "a".repeat(64)).is_err());
    assert!(job.begin_publish().is_err());
    assert!(job.mark_published().is_err());
    assert_eq!(AssetFormat::Mp4.extension(), "mp4");
    Ok(())
}

#[cfg(unix)]
#[test]
fn recovery_rejects_a_destination_symlink() -> Result<(), String> {
    use std::os::unix::fs::symlink;

    let directory = tempdir().map_err(|error| error.to_string())?;
    let target = directory.path().join("target");
    std::fs::create_dir(&target).map_err(|error| error.to_string())?;
    std::fs::create_dir(target.join("nested")).map_err(|error| error.to_string())?;
    let link = directory.path().join("destination-link");
    symlink(&target, &link).map_err(|error| error.to_string())?;
    assert!(ManualDestination::open("destination".to_owned(), link.clone()).is_err());
    assert!(ManualDestination::open("destination".to_owned(), link.join("nested")).is_err());
    Ok(())
}

#[test]
fn recovery_preserves_failed_jobs_for_integrity_and_format_retries() -> Result<(), String> {
    let directory = tempdir().map_err(|error| error.to_string())?;
    let mut mismatch = prepared_job(directory.path().to_path_buf(), &png())
        .map_err(|error| format!("job: {error:?}"))?;
    assert_eq!(
        AssetPublisher::publish(&mut mismatch, &mut BytesSource::new(b"other".to_vec())),
        Err(AssetPublishError::IntegrityMismatch)
    );

    let invalid = b"not-a-png".to_vec();
    let mut format = prepared_job(directory.path().to_path_buf(), &invalid)
        .map_err(|error| format!("format job: {error:?}"))?;
    assert_eq!(
        AssetPublisher::publish(&mut format, &mut BytesSource::new(invalid)),
        Err(AssetPublishError::InvalidFormat)
    );

    let payload = b"x".to_vec();
    let mut empty = prepared_job(directory.path().to_path_buf(), &payload)
        .map_err(|error| format!("empty job: {error:?}"))?;
    assert_eq!(
        AssetPublisher::publish(
            &mut empty,
            &mut EmptyChunkSource {
                sha256: sha256_hex(&payload),
                sent: false,
            },
        ),
        Err(AssetPublishError::InvalidChunk)
    );
    Ok(())
}

fn bound_service(
    root: &std::path::Path,
) -> Result<(super::AssetPublishService, AssetJob, std::path::PathBuf), String> {
    let output = root.join("published");
    std::fs::create_dir(&output).map_err(|error| error.to_string())?;
    let job = prepare_manual_asset_job(
        "recovered-job".to_owned(),
        AssetFormat::Png,
        "selected-folder".to_owned(),
        output.clone(),
    )
    .map_err(|error| format!("{error:?}"))?;
    let service = super::AssetPublishService::open(root.join("jobs"))
        .map_err(|error| format!("{error:?}"))?;
    service
        .save_prepared(&job)
        .map_err(|error| format!("{error:?}"))?;
    service
        .bind_prepared(job.id())
        .map_err(|error| format!("{error:?}"))?;
    let path = output.join(job.destination_leaf());
    Ok((service, job, path))
}

#[test]
fn publication_recovery_verifies_existing_bytes_and_reuses_receipt_without_reading_source()
-> Result<(), String> {
    let directory = tempdir().map_err(|error| error.to_string())?;
    let (service, job, path) = bound_service(directory.path())?;
    let mut bytes = png();
    bytes.resize(150_000, 7);
    let first = service
        .publish_finalized(job.id(), &mut BytesSource::new(bytes.clone()))
        .map_err(|error| format!("first: {error:?}"))?;
    let mut unavailable_source = BytesSource::failing(bytes.clone());
    let recovered = service
        .publish_finalized(job.id(), &mut unavailable_source)
        .map_err(|error| format!("recover: {error:?}"))?;
    assert_eq!(recovered, first);
    assert_eq!(unavailable_source.offset, 0);
    assert_eq!(
        std::fs::read(path).map_err(|error| error.to_string())?,
        bytes
    );
    let stored = AssetJobStore::open(directory.path().join("jobs"))
        .map_err(|error| format!("{error:?}"))?
        .load(job.id())
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(
        stored.map(|job| job.state()),
        Some(super::AssetJobState::Published)
    );
    Ok(())
}

#[test]
fn publication_recovery_never_replaces_collisions_or_accepts_changed_source_metadata()
-> Result<(), String> {
    use super::super::AssetPublishServiceError;
    let directory = tempdir().map_err(|error| error.to_string())?;
    let (service, job, path) = bound_service(directory.path())?;
    let bytes = png();
    service
        .publish_finalized(job.id(), &mut BytesSource::new(bytes.clone()))
        .map_err(|error| format!("first: {error:?}"))?;
    for replacement in [b"different-length".to_vec(), vec![b'x'; bytes.len()]] {
        std::fs::write(&path, &replacement).map_err(|error| error.to_string())?;
        assert!(matches!(
            service.publish_finalized(job.id(), &mut BytesSource::new(bytes.clone())),
            Err(AssetPublishServiceError::Publish(
                AssetPublishError::NameCollision | AssetPublishError::IntegrityMismatch
            ))
        ));
        assert_eq!(
            std::fs::read(&path).map_err(|error| error.to_string())?,
            replacement
        );
    }
    assert!(matches!(
        service.publish_finalized(job.id(), &mut BytesSource::new(b"changed-source".to_vec())),
        Err(AssetPublishServiceError::Store(_))
    ));
    std::fs::remove_file(&path).map_err(|error| error.to_string())?;
    std::fs::create_dir(&path).map_err(|error| error.to_string())?;
    assert_eq!(
        service.publish_finalized(job.id(), &mut BytesSource::new(bytes)),
        Err(AssetPublishServiceError::Publish(
            AssetPublishError::NameCollision
        ))
    );
    Ok(())
}

#[cfg(unix)]
#[test]
fn publication_recovery_refuses_an_existing_leaf_symlink() -> Result<(), String> {
    use super::super::AssetPublishServiceError;
    let directory = tempdir().map_err(|error| error.to_string())?;
    let (service, job, path) = bound_service(directory.path())?;
    let bytes = png();
    let outside = directory.path().join("outside.png");
    std::fs::write(&outside, &bytes).map_err(|error| error.to_string())?;
    std::os::unix::fs::symlink(&outside, &path).map_err(|error| error.to_string())?;
    assert_eq!(
        service.publish_finalized(job.id(), &mut BytesSource::new(bytes.clone())),
        Err(AssetPublishServiceError::Publish(
            AssetPublishError::NameCollision
        ))
    );
    assert_eq!(
        std::fs::read(&outside).map_err(|error| error.to_string())?,
        bytes
    );
    Ok(())
}
