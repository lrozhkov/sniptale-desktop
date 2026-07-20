//! Manual operation route binding before any media side effect.

use std::path::PathBuf;

use super::{AssetFormat, AssetJob, AssetJobError, AssetJobId, ManualDestination};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AssetRouteBindError {
    InvalidDestination,
    InvalidJob,
}

pub(crate) fn prepare_manual_asset_job(
    asset_job_id: String,
    format: AssetFormat,
    destination_id: String,
    destination_root: PathBuf,
) -> Result<AssetJob, AssetRouteBindError> {
    let destination = ManualDestination::open(destination_id, destination_root)
        .map_err(|_| AssetRouteBindError::InvalidDestination)?;
    let job_id = AssetJobId::parse(asset_job_id).map_err(|_| AssetRouteBindError::InvalidJob)?;
    let leaf = format!("{}.{}", job_id.as_str(), format.extension());
    AssetJob::prepare(job_id, format, &destination, leaf).map_err(map_job_error)
}

const fn map_job_error(_error: AssetJobError) -> AssetRouteBindError {
    AssetRouteBindError::InvalidJob
}
