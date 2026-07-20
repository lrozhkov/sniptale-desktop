use super::super::writable::{
    WritableDirectoryInputs, WritableDirectoryStatus, writable_directory_report,
};
use crate::diagnostics::runtime_access::{
    RuntimeDirectoryAccessFact, RuntimeDirectoryAccessReport, RuntimeDirectoryAccessStatus,
    runtime_directory_access_report_with_probe,
};

#[test]
fn writable_directory_report_checks_expected_categories() -> Result<(), Box<dyn std::error::Error>>
{
    let root = tempfile::tempdir()?;
    let report = writable_directory_report(&WritableDirectoryInputs {
        local_app_data_dir: Some(root.path().join("local-app-data")),
        temp_dir: root.path().join("temp"),
        process_id: 7,
    });

    assert!(report.all_writable());
    assert_eq!(
        report
            .facts
            .iter()
            .map(|fact| fact.category)
            .collect::<Vec<_>>(),
        vec!["logs", "diagnostics", "temp-media", "cache"]
    );
    assert!(report.failed_categories().is_empty());
    Ok(())
}

#[test]
fn writable_directory_report_reports_category_only_failures()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let blocked = root.path().join("blocked");
    std::fs::write(&blocked, b"not a directory")?;

    let report = writable_directory_report(&WritableDirectoryInputs {
        local_app_data_dir: Some(blocked),
        temp_dir: root.path().join("temp"),
        process_id: 7,
    });

    assert!(!report.all_writable());
    assert_eq!(
        report.failed_categories(),
        vec!["logs", "temp-media", "cache"]
    );
    assert!(
        report
            .facts
            .iter()
            .any(|fact| fact.category == "diagnostics"
                && fact.status == WritableDirectoryStatus::Writable)
    );
    Ok(())
}

#[test]
fn writable_directory_report_falls_back_to_temp_root_without_local_app_data()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let report = writable_directory_report(&WritableDirectoryInputs {
        local_app_data_dir: None,
        temp_dir: root.path().join("temp"),
        process_id: 7,
    });

    assert!(report.all_writable());
    assert!(report.facts.iter().all(|fact| !fact.category.contains('/')));
    assert!(
        report
            .facts
            .iter()
            .all(|fact| !fact.category.contains('\\'))
    );
    Ok(())
}

#[test]
fn runtime_directory_access_report_uses_same_category_plan()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let report = runtime_directory_access_report_with_probe(
        &WritableDirectoryInputs {
            local_app_data_dir: Some(root.path().join("local-app-data")),
            temp_dir: root.path().join("temp"),
            process_id: 7,
        },
        |_| RuntimeDirectoryAccessStatus::Protected,
    );

    assert!(!report.repair_required());
    assert_eq!(
        report
            .facts
            .iter()
            .map(|fact| fact.category)
            .collect::<Vec<_>>(),
        vec!["logs", "diagnostics", "temp-media", "cache"]
    );
    Ok(())
}

#[test]
fn runtime_directory_access_report_fails_closed_for_temp_media_broad_read() {
    let report = RuntimeDirectoryAccessReport {
        facts: vec![RuntimeDirectoryAccessFact {
            category: "temp-media",
            status: RuntimeDirectoryAccessStatus::BroadReadAccess,
        }],
    };

    assert!(report.repair_required());
    assert_eq!(
        report.unprotected_categories(),
        vec!["temp-media=broad-read-access"]
    );
}
