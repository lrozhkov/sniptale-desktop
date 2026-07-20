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
