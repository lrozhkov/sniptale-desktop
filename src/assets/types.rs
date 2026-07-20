//! Narrow identifiers and format values for local publication jobs.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct AssetJobId(String);

impl AssetJobId {
    pub(crate) fn parse(value: String) -> Result<Self, AssetTypeError> {
        if value.is_empty()
            || value.len() > 96
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(AssetTypeError::InvalidJobId);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AssetFormat {
    Png,
    Mp4,
}

impl AssetFormat {
    #[must_use]
    pub(crate) const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Mp4 => "mp4",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AssetTypeError {
    InvalidJobId,
}
