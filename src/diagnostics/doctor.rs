//! Install and runtime self-check owner.

use serde::Serialize;

use super::log::{DiagnosticOutput, redact_for_output};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DoctorCheckName {
    BinarySignature,
    RollbackProtection,
    InstallDirectoryAcl,
    NativeHostManifest,
    LocalProfile,
    #[cfg(any(windows, test))]
    ManualDestination,
    Autostart,
    RuntimeDirectoryAccess,
    RuntimeStartup,
    WritableDirectories,
    EncoderAvailability,
    RecordingAvailability,
    CaptureAvailability,
    AudioAvailability,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DoctorStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DoctorCheck {
    pub(crate) name: DoctorCheckName,
    pub(crate) status: DoctorStatus,
    pub(crate) detail: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub(crate) categories: Vec<DoctorCategoryStatus>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DoctorCategoryStatus {
    pub(crate) category: &'static str,
    pub(crate) status: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DoctorReport {
    pub(crate) checks: Vec<DoctorCheck>,
    pub(crate) repair_required: bool,
}

pub(crate) fn doctor_report(checks: Vec<DoctorCheck>) -> DoctorReport {
    let repair_required = checks
        .iter()
        .any(|check| check.status == DoctorStatus::Fail);
    DoctorReport {
        checks,
        repair_required,
    }
}

pub(crate) fn doctor_check(
    name: DoctorCheckName,
    status: DoctorStatus,
    raw_detail: &str,
) -> DoctorCheck {
    doctor_check_with_categories(name, status, raw_detail, Vec::new())
}

pub(crate) fn doctor_check_with_categories(
    name: DoctorCheckName,
    status: DoctorStatus,
    raw_detail: &str,
    categories: Vec<DoctorCategoryStatus>,
) -> DoctorCheck {
    DoctorCheck {
        name,
        status,
        detail: redact_for_output(
            DiagnosticOutput::DoctorJson,
            raw_detail,
            "install-health",
            "doctor",
        )
        .message,
        categories,
    }
}
