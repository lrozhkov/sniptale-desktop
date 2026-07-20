//! Runtime directory access doctor check formatting.

use crate::diagnostics::{
    DoctorCategoryStatus, DoctorCheck, DoctorCheckName, DoctorStatus, RuntimeDirectoryAccessReport,
    doctor_check_with_categories,
};

pub(super) fn runtime_directory_access_doctor_check(
    report: &RuntimeDirectoryAccessReport,
) -> DoctorCheck {
    let detail = runtime_directory_access_doctor_detail(report);
    doctor_check_with_categories(
        DoctorCheckName::RuntimeDirectoryAccess,
        runtime_directory_access_doctor_status(report),
        &detail,
        runtime_directory_access_categories(report),
    )
}

fn runtime_directory_access_doctor_status(report: &RuntimeDirectoryAccessReport) -> DoctorStatus {
    if report.repair_required() {
        DoctorStatus::Fail
    } else {
        DoctorStatus::Pass
    }
}

fn runtime_directory_access_doctor_detail(report: &RuntimeDirectoryAccessReport) -> String {
    let unprotected = report.unprotected_categories();
    if unprotected.is_empty() {
        "runtime directory access is protected".to_owned()
    } else {
        format!(
            "runtime directory access requires repair: {}",
            unprotected.join(",")
        )
    }
}

fn runtime_directory_access_categories(
    report: &RuntimeDirectoryAccessReport,
) -> Vec<DoctorCategoryStatus> {
    report
        .facts
        .iter()
        .map(|fact| DoctorCategoryStatus {
            category: fact.category,
            status: fact.status.as_str(),
        })
        .collect()
}
