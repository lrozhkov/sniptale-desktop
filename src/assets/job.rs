//! Versioned private state for a single Manual artifact publication.

use serde::{Deserialize, Serialize};

use super::{AssetFormat, AssetJobId, ManualDestination};

const ASSET_JOB_SCHEMA_VERSION: u16 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AssetJobState {
    Prepared,
    BoundAwaitingArtifact,
    Pending,
    Publishing,
    Published,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AssetJob {
    schema_version: u16,
    job_id: AssetJobId,
    format: AssetFormat,
    destination_id: String,
    destination_root: String,
    destination_leaf: String,
    expected_bytes: Option<u64>,
    expected_sha256: Option<String>,
    state: AssetJobState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AssetJobError {
    InvalidState,
    InvalidMetadata,
}

impl AssetJob {
    pub(crate) fn prepare(
        job_id: AssetJobId,
        format: AssetFormat,
        destination: &ManualDestination,
        destination_leaf: String,
    ) -> Result<Self, AssetJobError> {
        let job = Self {
            schema_version: ASSET_JOB_SCHEMA_VERSION,
            job_id,
            format,
            destination_id: destination.id().to_owned(),
            destination_root: destination.root().to_string_lossy().into_owned(),
            destination_leaf,
            expected_bytes: None,
            expected_sha256: None,
            state: AssetJobState::Prepared,
        };
        job.validate()?;
        Ok(job)
    }

    #[cfg(test)]
    pub(crate) fn bind_artifact(
        &mut self,
        expected_bytes: u64,
        expected_sha256: String,
    ) -> Result<(), AssetJobError> {
        self.bind_operation()?;
        self.attach_finalized_artifact(expected_bytes, expected_sha256)
    }

    /// Acknowledges the frozen Manual route before the capture or recording owner may create media.
    pub(crate) fn bind_operation(&mut self) -> Result<(), AssetJobError> {
        if self.state != AssetJobState::Prepared {
            return Err(AssetJobError::InvalidState);
        }
        self.state = AssetJobState::BoundAwaitingArtifact;
        self.validate()
    }

    /// Binds the hash and size observed only after the protected artifact was finalized.
    pub(crate) fn attach_finalized_artifact(
        &mut self,
        expected_bytes: u64,
        expected_sha256: String,
    ) -> Result<(), AssetJobError> {
        if self.state != AssetJobState::BoundAwaitingArtifact
            || expected_bytes == 0
            || !valid_sha256(&expected_sha256)
        {
            return Err(AssetJobError::InvalidState);
        }
        self.expected_bytes = Some(expected_bytes);
        self.expected_sha256 = Some(expected_sha256);
        self.state = AssetJobState::Pending;
        self.validate()
    }

    pub(crate) fn begin_publish(&mut self) -> Result<(), AssetJobError> {
        if self.state != AssetJobState::Pending && self.state != AssetJobState::Failed {
            return Err(AssetJobError::InvalidState);
        }
        self.state = AssetJobState::Publishing;
        Ok(())
    }

    pub(crate) fn mark_published(&mut self) -> Result<(), AssetJobError> {
        if self.state != AssetJobState::Publishing {
            return Err(AssetJobError::InvalidState);
        }
        self.state = AssetJobState::Published;
        Ok(())
    }

    pub(crate) fn mark_failed(&mut self) {
        if self.state == AssetJobState::Publishing {
            self.state = AssetJobState::Failed;
        }
    }

    pub(crate) fn validate(&self) -> Result<(), AssetJobError> {
        if self.schema_version != ASSET_JOB_SCHEMA_VERSION
            || self.destination_id.is_empty()
            || self.destination_id.len() > 128
            || self.destination_root.is_empty()
            || !valid_leaf(&self.destination_leaf, self.format)
        {
            return Err(AssetJobError::InvalidMetadata);
        }
        match self.state {
            AssetJobState::Prepared | AssetJobState::BoundAwaitingArtifact => {
                if self.expected_bytes.is_some() || self.expected_sha256.is_some() {
                    return Err(AssetJobError::InvalidMetadata);
                }
            }
            AssetJobState::Pending
            | AssetJobState::Publishing
            | AssetJobState::Published
            | AssetJobState::Failed => {
                if self.expected_bytes.is_none_or(|bytes| bytes == 0)
                    || self
                        .expected_sha256
                        .as_deref()
                        .is_none_or(|hash| !valid_sha256(hash))
                {
                    return Err(AssetJobError::InvalidMetadata);
                }
            }
        }
        Ok(())
    }

    #[must_use]
    pub(crate) fn id(&self) -> &AssetJobId {
        &self.job_id
    }

    #[must_use]
    pub(crate) const fn format(&self) -> AssetFormat {
        self.format
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) const fn state(&self) -> AssetJobState {
        self.state
    }

    #[must_use]
    pub(crate) fn expected_sha256(&self) -> Option<&str> {
        self.expected_sha256.as_deref()
    }

    #[must_use]
    pub(crate) const fn expected_bytes(&self) -> Option<u64> {
        self.expected_bytes
    }

    #[must_use]
    pub(crate) fn destination_leaf(&self) -> &str {
        &self.destination_leaf
    }

    #[must_use]
    pub(crate) fn destination_id(&self) -> &str {
        &self.destination_id
    }

    #[must_use]
    pub(crate) fn destination_root(&self) -> &str {
        &self.destination_root
    }
}

fn valid_leaf(value: &str, format: AssetFormat) -> bool {
    let expected_suffix = format!(".{}", format.extension());
    !value.is_empty()
        && value.len() <= 180
        && value.ends_with(&expected_suffix)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && !value.starts_with('.')
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
