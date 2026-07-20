//! Redacted local-profile health projection for doctor output.

use std::path::PathBuf;

use crate::{
    diagnostics::{DoctorCheck, DoctorCheckName, DoctorStatus, doctor_check},
    settings::{LocalProfileHealth, OperatingMode, local_profile_health_from_path},
};

pub(super) fn local_profile_doctor_check(path: Option<PathBuf>) -> DoctorCheck {
    let profile = local_profile_health_from_path(path);
    doctor_check(
        DoctorCheckName::LocalProfile,
        local_profile_doctor_status(profile),
        local_profile_doctor_detail(profile),
    )
}

const fn local_profile_doctor_status(profile: LocalProfileHealth) -> DoctorStatus {
    match profile {
        LocalProfileHealth::Missing | LocalProfileHealth::Available(_) => DoctorStatus::Pass,
        LocalProfileHealth::RepairRequired(_) => DoctorStatus::Fail,
    }
}

const fn local_profile_doctor_detail(profile: LocalProfileHealth) -> &'static str {
    match profile {
        LocalProfileHealth::Missing => {
            "local profile is absent; extension-managed mode is selected"
        }
        LocalProfileHealth::Available(OperatingMode::ExtensionManaged) => {
            "local profile selects extension-managed mode"
        }
        LocalProfileHealth::Available(OperatingMode::Manual) => "local profile selects manual mode",
        LocalProfileHealth::RepairRequired(_) => "local profile requires repair",
    }
}
