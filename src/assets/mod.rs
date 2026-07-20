//! Durable local publication jobs; Manual-mode artifacts.

mod destination;
mod job;
mod publisher;
mod route;
mod service;
mod source;
mod store;
mod types;

pub(crate) use destination::ManualDestination;
#[cfg(test)]
pub(crate) use destination::ManualDestinationError;
#[cfg(test)]
pub(crate) use job::AssetJobState;
pub(crate) use job::{AssetJob, AssetJobError};
pub(crate) use publisher::{AssetPublishError, AssetPublisher, PublishReceipt};
pub(crate) use route::prepare_manual_asset_job;
pub(crate) use service::{AssetPublishService, AssetPublishServiceError};
pub(crate) use source::{FinalizedArtifactSource, FinalizedArtifactSourceError};
pub(crate) use store::{AssetJobStore, AssetJobStoreError};
pub(crate) use types::{AssetFormat, AssetJobId};

#[cfg(test)]
mod tests;
