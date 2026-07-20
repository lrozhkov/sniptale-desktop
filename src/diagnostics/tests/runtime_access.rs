use super::super::runtime_access::{
    RuntimeDirectoryAccessFact, RuntimeDirectoryAccessReport, RuntimeDirectoryAccessStatus,
};
use super::super::{DoctorCheckName, DoctorStatus, doctor_check_with_categories};

#[test]
fn runtime_access_category_statuses_do_not_expose_paths() -> Result<(), serde_json::Error> {
    let report = RuntimeDirectoryAccessReport {
        facts: vec![RuntimeDirectoryAccessFact {
            category: "temp-media",
            status: RuntimeDirectoryAccessStatus::BroadWriteAccess,
        }],
    };
    let categories = report
        .facts
        .iter()
        .map(|fact| super::super::DoctorCategoryStatus {
            category: fact.category,
            status: fact.status.as_str(),
        })
        .collect();
    let check = doctor_check_with_categories(
        DoctorCheckName::RuntimeDirectoryAccess,
        DoctorStatus::Fail,
        "runtime directory access requires repair: temp-media=broad-write-access",
        categories,
    );
    let serialized = serde_json::to_string(&check)?;

    assert!(serialized.contains("\"category\":\"temp-media\""));
    assert!(serialized.contains("\"status\":\"broad-write-access\""));
    assert!(!serialized.contains(":\\\\Users\\\\"));
    assert!(!serialized.contains('/'));
    Ok(())
}
