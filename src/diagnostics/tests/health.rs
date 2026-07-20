use std::fs;

use super::super::bundle::{BundleCandidate, BundleEntryKind, plan_bundle, write_bundle_to_dir};
use super::super::doctor::{DoctorCheckName, DoctorStatus, doctor_check, doctor_report};
use super::super::health::{
    CapabilityHealth, ConnectionHealth, HealthSnapshot, InstallHealth, LastErrorSummary,
    ProcessMode,
};

fn healthy_snapshot() -> HealthSnapshot {
    HealthSnapshot {
        app_version: "0.1.0".to_owned(),
        protocol_version: 1,
        settings_schema_version: 1,
        cache_schema_version: 1,
        process_mode: ProcessMode::Agent,
        connection: ConnectionHealth::Connected,
        install: InstallHealth::Healthy,
        capture: CapabilityHealth::Available,
        recording: CapabilityHealth::Available,
        audio: CapabilityHealth::Degraded,
        last_error: None,
    }
}

#[test]
fn health_snapshot_reports_operational_and_repair_required_states() {
    let snapshot = healthy_snapshot();
    assert!(snapshot.operational());
    assert!(!snapshot.repair_required());

    let mut repair = snapshot;
    repair.install = InstallHealth::RepairRequired;
    repair.last_error = Some(LastErrorSummary {
        code: "repair-required".to_owned(),
        phase: "install-health".to_owned(),
        recoverable: true,
    });

    assert!(!repair.operational());
    assert!(repair.repair_required());
}

#[test]
fn doctor_report_fails_closed_and_redacts_details() {
    let report = doctor_report(vec![
        doctor_check(
            DoctorCheckName::BinarySignature,
            DoctorStatus::Pass,
            "signature valid",
        ),
        doctor_check(
            DoctorCheckName::NativeHostManifest,
            DoctorStatus::Fail,
            "manifest at C:\\Users\\Alice\\Private\\native.json has token secret-token",
        ),
    ]);

    assert!(report.repair_required);
    assert_eq!(report.checks[1].status, DoctorStatus::Fail);
    assert!(!report.checks[1].detail.contains("C:\\Users\\Alice"));
    assert!(!report.checks[1].detail.contains("secret-token"));
}

#[test]
fn support_bundle_excludes_raw_payload_categories_and_redacts_included_entries() {
    let plan = plan_bundle(&[
        BundleCandidate {
            kind: BundleEntryKind::DoctorJson,
            name: "doctor.json".to_owned(),
            content: "{\"type\":\"extension.diagnostics.collect\",\"extensionId\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\",\"recordingId\":\"recording-secret-9\",\"path\":\"C:\\Users\\Alice\\Private\\capture.mp4\"}".to_owned(),
        },
        BundleCandidate {
            kind: BundleEntryKind::MediaPayload,
            name: "recording.mp4".to_owned(),
            content: "AAAA_SECRET_MEDIA_BYTES".to_owned(),
        },
        BundleCandidate {
            kind: BundleEntryKind::TextInputBuffer,
            name: "typed.txt".to_owned(),
            content: "password123".to_owned(),
        },
        BundleCandidate {
            kind: BundleEntryKind::TempMedia,
            name: "temp-media.mp4".to_owned(),
            content: "data:video/mp4;base64,AAAA_SECRET_MEDIA_BYTES".to_owned(),
        },
    ]);

    assert_eq!(plan.entries.len(), 1);
    assert_eq!(plan.excluded_count, 3);
    assert_eq!(plan.entries[0].kind, BundleEntryKind::DoctorJson);
    assert!(
        plan.entries[0]
            .redacted_content
            .contains("message-type=extension.diagnostics.collect")
    );
    assert!(!plan.entries[0].redacted_content.contains("aaaaaaaa"));
    assert!(
        !plan.entries[0]
            .redacted_content
            .contains("recording-secret-9")
    );
    assert!(
        !plan.entries[0]
            .redacted_content
            .contains("C:\\Users\\Alice")
    );
}

#[test]
fn support_bundle_writer_persists_redacted_json_without_raw_entry_names()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let result = write_bundle_to_dir(
        directory.path(),
        1_234,
        &[
            BundleCandidate {
                kind: BundleEntryKind::DoctorJson,
                name: "C:\\Users\\Alice\\doctor.json".to_owned(),
                content: "token secret-token at C:\\Users\\Alice\\Private".to_owned(),
            },
            BundleCandidate {
                kind: BundleEntryKind::VersionJson,
                name: "version.json".to_owned(),
                content: "{\"appVersion\":\"0.1.0\"}".to_owned(),
            },
            BundleCandidate {
                kind: BundleEntryKind::RawLog,
                name: "raw.log".to_owned(),
                content: "typed text: password123 clipboard: copied-secret".to_owned(),
            },
        ],
    )?;

    assert_eq!(result.entries_written, 2);
    assert_eq!(result.excluded_count, 1);
    assert!(result.path.starts_with(directory.path()));
    assert!(result.file_name.starts_with("sniptale-diagnostics-1234-"));

    let bundle_bytes = fs::read(&result.path)?;
    assert!(bundle_bytes.starts_with(b"PK\x03\x04"));
    let bundle_text = String::from_utf8_lossy(&bundle_bytes);
    assert!(bundle_text.contains("\"formatVersion\": 1"));
    assert!(bundle_text.contains("\"excludedCount\": 1"));
    assert!(!bundle_text.contains("C:\\Users\\Alice"));
    assert!(!bundle_text.contains("secret-token"));
    assert!(!bundle_text.contains("password123"));
    assert!(bundle_text.contains("manifest.json"));
    assert!(bundle_text.contains("redacted-entry.txt"));
    assert!(bundle_text.contains("version.json"));
    Ok(())
}
