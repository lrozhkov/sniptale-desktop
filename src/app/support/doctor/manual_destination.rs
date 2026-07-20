//! Redacted Manual destination health projection for doctor output.

use std::path::PathBuf;

use crate::{
    assets::ManualDestination,
    diagnostics::{
        DoctorCategoryStatus, DoctorCheck, DoctorCheckName, DoctorStatus,
        doctor_check_with_categories,
    },
    settings::{OperatingMode, load_local_profile_from_path},
};

pub(super) fn manual_destination_doctor_check(path: Option<PathBuf>) -> DoctorCheck {
    let (status, detail, category) = match load_local_profile_from_path(path) {
        Ok(None) => (
            DoctorStatus::Pass,
            "manual destination is not configured in extension-managed mode",
            "not-configured",
        ),
        Ok(Some(profile)) if profile.operating_mode() == OperatingMode::ExtensionManaged => (
            DoctorStatus::Pass,
            "manual destination is inactive in extension-managed mode",
            "inactive",
        ),
        Ok(Some(profile)) => match profile.manual_destination() {
            Some(destination) => manual_destination_status(destination.id, destination.root),
            None => (
                DoctorStatus::Fail,
                "manual destination is missing",
                "missing",
            ),
        },
        Err(_) => (
            DoctorStatus::Fail,
            "manual destination cannot be evaluated until local profile repair",
            "profile-repair-required",
        ),
    };
    doctor_check_with_categories(
        DoctorCheckName::ManualDestination,
        status,
        detail,
        vec![DoctorCategoryStatus {
            category: "destination",
            status: category,
        }],
    )
}

fn manual_destination_status(
    id: String,
    root: PathBuf,
) -> (DoctorStatus, &'static str, &'static str) {
    if ManualDestination::open(id, root).is_ok() {
        (
            DoctorStatus::Pass,
            "manual destination is available",
            "available",
        )
    } else {
        (
            DoctorStatus::Fail,
            "manual destination is unavailable",
            "unavailable",
        )
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::manual_destination_doctor_check;
    use crate::{
        diagnostics::{DoctorCheckName, DoctorStatus},
        settings::{
            LocalProfileDraft, ManualProfileDraft, apply_local_profile_draft_from_path,
            ensure_local_profile_from_path,
        },
    };

    #[test]
    fn manual_destination_doctor_check_is_category_only() -> Result<(), Box<dyn std::error::Error>>
    {
        let directory = tempdir()?;
        let profile_path = directory.path().join("local-profile.json");
        let profile = ensure_local_profile_from_path(Some(profile_path.clone()))
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        let settings = crate::settings::ActiveSettings::from_profile(&profile);
        let draft = LocalProfileDraft::manual(ManualProfileDraft::with_defaults(
            "destination".to_owned(),
            directory.path().to_string_lossy().into_owned(),
        ));
        apply_local_profile_draft_from_path(Some(profile_path.clone()), &settings, &draft)
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;

        let check = manual_destination_doctor_check(Some(profile_path));

        assert_eq!(check.name, DoctorCheckName::ManualDestination);
        assert_eq!(check.status, DoctorStatus::Pass);
        assert_eq!(check.categories[0].category, "destination");
        assert_eq!(check.categories[0].status, "available");
        assert!(
            !check
                .detail
                .contains(&directory.path().to_string_lossy().into_owned())
        );
        Ok(())
    }
}
