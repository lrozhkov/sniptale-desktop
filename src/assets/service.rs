//! Durable publication service joining a private job store and bounded publisher.

use std::path::PathBuf;

use super::{
    AssetJob, AssetJobId, AssetJobStore, AssetJobStoreError, AssetPublishError, AssetPublisher,
    FinalizedArtifactSource, PublishReceipt,
};

#[derive(Debug)]
pub(crate) struct AssetPublishService {
    store: AssetJobStore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AssetPublishServiceError {
    Store(AssetJobStoreError),
    Publish(AssetPublishError),
}

impl AssetPublishService {
    pub(crate) fn open(root: PathBuf) -> Result<Self, AssetPublishServiceError> {
        Ok(Self {
            store: AssetJobStore::open(root).map_err(AssetPublishServiceError::Store)?,
        })
    }

    pub(crate) fn publish(
        &self,
        job: &mut AssetJob,
        source: &mut dyn FinalizedArtifactSource,
    ) -> Result<PublishReceipt, AssetPublishServiceError> {
        self.store
            .save(job)
            .map_err(AssetPublishServiceError::Store)?;
        let result = AssetPublisher::publish(job, source);
        self.store
            .save(job)
            .map_err(AssetPublishServiceError::Store)?;
        result.map_err(AssetPublishServiceError::Publish)
    }

    pub(crate) fn save_prepared(&self, job: &AssetJob) -> Result<(), AssetPublishServiceError> {
        self.store
            .save(job)
            .map_err(AssetPublishServiceError::Store)
    }

    /// Persists the pre-media route acknowledgement for an already durable Prepared job.
    pub(crate) fn bind_prepared(
        &self,
        job_id: &AssetJobId,
    ) -> Result<AssetJob, AssetPublishServiceError> {
        let mut job = self.load(job_id)?;
        job.bind_operation()
            .map_err(|_| AssetPublishServiceError::Store(AssetJobStoreError::InvalidJob))?;
        self.store
            .save(&job)
            .map_err(AssetPublishServiceError::Store)?;
        Ok(job)
    }

    /// Rehydrates a route-bound job, attaches finalized artifact facts, and publishes it.
    pub(crate) fn publish_finalized(
        &self,
        job_id: &AssetJobId,
        source: &mut dyn FinalizedArtifactSource,
    ) -> Result<PublishReceipt, AssetPublishServiceError> {
        let mut job = self.load(job_id)?;
        job.attach_finalized_artifact(source.total_bytes(), source.sha256().to_owned())
            .map_err(|_| AssetPublishServiceError::Store(AssetJobStoreError::InvalidJob))?;
        self.publish(&mut job, source)
    }

    fn load(&self, job_id: &AssetJobId) -> Result<AssetJob, AssetPublishServiceError> {
        self.store
            .load(job_id)
            .map_err(AssetPublishServiceError::Store)?
            .ok_or(AssetPublishServiceError::Store(
                AssetJobStoreError::InvalidJob,
            ))
    }
}
