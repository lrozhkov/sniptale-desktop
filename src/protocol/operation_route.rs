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
    LocalAsset { asset_job_id: String },
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
