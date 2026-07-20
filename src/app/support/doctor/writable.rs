//! Redacted writable-runtime-directory doctor projection.

use crate::diagnostics::{DoctorStatus, WritableDirectoryReport};

pub(super) fn writable_directories_doctor_status(report: &WritableDirectoryReport) -> DoctorStatus {
    if report.all_writable() {
        DoctorStatus::Pass
    } else {
        DoctorStatus::Fail
    }
}

pub(super) fn writable_directories_doctor_detail(report: &WritableDirectoryReport) -> String {
    let failed = report.failed_categories();
    if failed.is_empty() {
        "required directories are writable".to_owned()
    } else {
        format!("unwritable directory categories: {}", failed.join(","))
    }
}
