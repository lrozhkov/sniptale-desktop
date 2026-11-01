//! Platform-neutral operation authority and delivery route values.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum OperationAuthority {
    ExtensionManaged { settings_revision: String },
    Manual { profile_revision: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DeliveryRoute {
    ExtensionTransfer,
    LocalAsset {
        asset_job_id: String,
    },
    DesktopDelivery {
        asset_job_id: String,
        target_id: Option<String>,
        ask_after_capture: bool,
    },
}

#[cfg(any(windows, test))]
impl DeliveryRoute {
    pub(crate) fn valid_local_route(&self) -> bool {
        let valid_job = self.local_asset_job_id().is_some_and(|id| {
            !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        });
        valid_job
            && match self {
                Self::DesktopDelivery {
                    target_id: Some(id),
                    ..
                } => {
                    id.len() == 64
                        && id
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                }
                Self::LocalAsset { .. }
                | Self::DesktopDelivery {
                    target_id: None, ..
                } => true,
                Self::ExtensionTransfer => false,
            }
    }
    pub(crate) fn local_asset_job_id(&self) -> Option<&str> {
        match self {
            Self::LocalAsset { asset_job_id } | Self::DesktopDelivery { asset_job_id, .. } => {
                Some(asset_job_id)
            }
            Self::ExtensionTransfer => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DeliveryRoute, OperationAuthority};

    #[test]
    fn manual_route_serialization_has_no_extension_controller() -> Result<(), String> {
        let authority = OperationAuthority::Manual {
            profile_revision: "profile-7".to_owned(),
        };
        let route = DeliveryRoute::LocalAsset {
            asset_job_id: "asset-job-7".to_owned(),
        };

        assert!(matches!(authority, OperationAuthority::Manual { .. }));
        let serialized = serde_json::to_string(&route).map_err(|error| error.to_string())?;
        assert!(!serialized.contains("lease"));
        Ok(())
    }
}
