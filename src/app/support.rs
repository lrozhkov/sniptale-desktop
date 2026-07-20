//! Support-mode JSON output for diagnostics, doctor, and version commands.

mod doctor;
mod runtime_access;

use std::io::Write;

use serde::Serialize;

use self::doctor::runtime_doctor_report;
#[cfg(test)]
pub(crate) use self::doctor::{recording_doctor_detail, recording_doctor_status};
use super::{
    host::{ModeError, ProcessMode},
    responses::runtime_capability_snapshot,
};
use crate::{
    diagnostics::{
        BundleCandidate, BundleEntryKind, BundleWriteError, BundleWriteResult, DoctorCheck,
        ObservabilitySnapshot, runtime_observability_json, runtime_observability_snapshot,
        runtime_recent_errors_json, write_runtime_bundle,
    },
    install::{INSTALLER_VERSION, NATIVE_HOST_MANIFEST_VERSION},
    install::{RepairRegistrationReport, current_repair_registration_report},
    protocol::{CACHE_SCHEMA_VERSION, PROTOCOL_VERSION, SETTINGS_SCHEMA_VERSION},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionJson {
    app_version: &'static str,
    protocol_version: u16,
    settings_schema_version: u16,
    app_cache_schema_version: u16,
    installer_version: &'static str,
    native_host_manifest_version: &'static str,
    platform_kind: &'static str,
    process_mode: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DoctorJson {
    process_mode: &'static str,
    status: &'static str,
    repair_required: bool,
    checks: Vec<DoctorCheck>,
    observability: ObservabilitySnapshot,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CollectDiagnosticsJson {
    process_mode: &'static str,
    status: &'static str,
    repair_required: bool,
    bundle_path_category: &'static str,
    bundle_file_name: String,
    entries_written: usize,
    excluded_entries: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestSummaryJson {
    installer_version: &'static str,
    native_host_manifest_version: &'static str,
    platform_kind: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RepairRegistrationJson {
    process_mode: &'static str,
    #[serde(flatten)]
    report: RepairRegistrationReport,
}

pub(super) fn write_version_json(stdout: &mut impl Write) -> Result<(), ModeError> {
    let payload = runtime_version_json();
    write_json_line(stdout, &payload)
}

pub(super) fn write_repair_registration_json(stdout: &mut impl Write) -> Result<(), ModeError> {
    let payload = RepairRegistrationJson {
        process_mode: ProcessMode::RepairRegistration.as_str(),
        report: current_repair_registration_report(),
    };
    write_json_line(stdout, &payload)
}

pub(super) fn write_doctor_json(stdout: &mut impl Write) -> Result<(), ModeError> {
    let payload = runtime_doctor_json();
    write_json_line(stdout, &payload)
}

pub(super) fn write_collect_diagnostics_json(stdout: &mut impl Write) -> Result<(), ModeError> {
    let doctor = runtime_doctor_json();
    let candidates = collect_diagnostics_candidates(&doctor)?;
    let bundle = write_runtime_bundle(&candidates).map_err(bundle_error_to_mode_error)?;
    let payload = collect_diagnostics_json(&doctor, &bundle);
    write_json_line(stdout, &payload)
}

fn write_json_line<T: Serialize>(stdout: &mut impl Write, payload: &T) -> Result<(), ModeError> {
    serde_json::to_writer(&mut *stdout, payload).map_err(|_| ModeError::Json)?;
    stdout.write_all(b"\n").map_err(|_| ModeError::Io)
}

fn runtime_version_json() -> VersionJson {
    VersionJson {
        app_version: env!("CARGO_PKG_VERSION"),
        protocol_version: PROTOCOL_VERSION,
        settings_schema_version: SETTINGS_SCHEMA_VERSION,
        app_cache_schema_version: CACHE_SCHEMA_VERSION,
        installer_version: INSTALLER_VERSION,
        native_host_manifest_version: NATIVE_HOST_MANIFEST_VERSION,
        platform_kind: platform_kind(),
        process_mode: ProcessMode::VersionJson.as_str(),
    }
}

fn runtime_doctor_json() -> DoctorJson {
    let report = runtime_doctor_report();
    DoctorJson {
        process_mode: ProcessMode::DoctorJson.as_str(),
        status: if report.repair_required {
            "fail"
        } else {
            "pass"
        },
        repair_required: report.repair_required,
        checks: report.checks,
        observability: runtime_observability_snapshot(),
    }
}

fn collect_diagnostics_json(
    doctor: &DoctorJson,
    bundle: &BundleWriteResult,
) -> CollectDiagnosticsJson {
    CollectDiagnosticsJson {
        process_mode: ProcessMode::CollectDiagnostics.as_str(),
        status: "bundle-written",
        repair_required: doctor.repair_required,
        bundle_path_category: "temp-diagnostics",
        bundle_file_name: bundle.file_name.clone(),
        entries_written: bundle.entries_written,
        excluded_entries: bundle.excluded_count,
    }
}

fn collect_diagnostics_candidates(doctor: &DoctorJson) -> Result<Vec<BundleCandidate>, ModeError> {
    Ok(vec![
        BundleCandidate {
            kind: BundleEntryKind::VersionJson,
            name: "version.json".to_owned(),
            content: serde_json::to_string(&runtime_version_json()).map_err(|_| ModeError::Json)?,
        },
        BundleCandidate {
            kind: BundleEntryKind::DoctorJson,
            name: "doctor.json".to_owned(),
            content: serde_json::to_string(doctor).map_err(|_| ModeError::Json)?,
        },
        BundleCandidate {
            kind: BundleEntryKind::ManifestSummary,
            name: "manifest-summary.json".to_owned(),
            content: serde_json::to_string(&runtime_manifest_summary())
                .map_err(|_| ModeError::Json)?,
        },
        BundleCandidate {
            kind: BundleEntryKind::CapabilitySnapshot,
            name: "capability-snapshot.json".to_owned(),
            content: serde_json::to_string(&runtime_capability_snapshot())
                .map_err(|_| ModeError::Json)?,
        },
        BundleCandidate {
            kind: BundleEntryKind::RecentErrorSummary,
            name: "recent-errors.json".to_owned(),
            content: runtime_recent_errors_json().map_err(|_| ModeError::Json)?,
        },
        BundleCandidate {
            kind: BundleEntryKind::ObservabilitySnapshot,
            name: "observability.json".to_owned(),
            content: runtime_observability_json().map_err(|_| ModeError::Json)?,
        },
    ])
}

fn runtime_manifest_summary() -> ManifestSummaryJson {
    ManifestSummaryJson {
        installer_version: INSTALLER_VERSION,
        native_host_manifest_version: NATIVE_HOST_MANIFEST_VERSION,
        platform_kind: platform_kind(),
    }
}

const fn bundle_error_to_mode_error(error: BundleWriteError) -> ModeError {
    match error {
        BundleWriteError::Json => ModeError::Json,
        BundleWriteError::Clock | BundleWriteError::Io => ModeError::Io,
    }
}

const fn platform_kind() -> &'static str {
    if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}
