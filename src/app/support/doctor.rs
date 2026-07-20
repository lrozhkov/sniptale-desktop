//! Runtime doctor report composition for support commands.

mod local_profile;
#[cfg(any(windows, test))]
mod manual_destination;
mod startup;
mod writable;

use crate::{
    app::resident::runtime_agent_startup_plan,
    audio::{AudioCapabilitySummary, runtime_audio_capability_summary},
    capture::{WgcRuntimeAvailability, runtime_capture_availability},
    diagnostics::{
        DoctorCheck, DoctorCheckName, DoctorReport, DoctorStatus, doctor_check, doctor_report,
        runtime_directory_access_report, runtime_writable_directory_report,
    },
    install::{
        InstallDirectoryAclStatus, RepairRegistrationReport, current_install_directory_acl_status,
        current_repair_registration_report, install_directory_acl_repair_required,
    },
    recording::{
        MediaFoundationAvailability, RecordingRuntimeAvailability, media_foundation_availability,
        recording_runtime_availability,
    },
    settings::{CacheSchemaStatus, NativeCacheHealth, native_cache_health_from_path},
};

use super::super::platform::{local_profile_path, native_cache_path};
use super::runtime_access::runtime_directory_access_doctor_check;
use local_profile::local_profile_doctor_check;
#[cfg(any(windows, test))]
use manual_destination::manual_destination_doctor_check;
use startup::startup_doctor_check;
use writable::{writable_directories_doctor_detail, writable_directories_doctor_status};

pub(super) fn runtime_doctor_report() -> DoctorReport {
    let capture_availability = runtime_capture_availability();
    let media_foundation_availability = media_foundation_availability();
    let recording_availability = recording_runtime_availability();
    let audio = runtime_audio_capability_summary();
    let install_directory_acl = current_install_directory_acl_status();
    let cache = native_cache_health_from_path(native_cache_path());
    let writable = runtime_writable_directory_report();
    let runtime_access = runtime_directory_access_report();
    let registration_report = current_repair_registration_report();
    let mut checks = install_repair_checks(&registration_report);
    checks.push(startup_doctor_check(runtime_agent_startup_plan().status));
    checks.push(doctor_check(
        DoctorCheckName::RollbackProtection,
        rollback_doctor_status(cache),
        rollback_doctor_detail(cache.schema_status),
    ));
    checks.push(local_profile_doctor_check(local_profile_path()));
    #[cfg(any(windows, test))]
    checks.push(manual_destination_doctor_check(local_profile_path()));
    checks.push(doctor_check(
        DoctorCheckName::InstallDirectoryAcl,
        install_directory_acl_doctor_status(install_directory_acl),
        install_directory_acl_doctor_detail(install_directory_acl),
    ));
    checks.push(runtime_directory_access_doctor_check(&runtime_access));
    let writable_detail = writable_directories_doctor_detail(&writable);
    checks.push(doctor_check(
        DoctorCheckName::WritableDirectories,
        writable_directories_doctor_status(&writable),
        &writable_detail,
    ));
    checks.extend(runtime_capability_checks(
        capture_availability,
        media_foundation_availability,
        recording_availability,
        &audio,
    ));
    doctor_report(checks)
}

fn runtime_capability_checks(
    capture_availability: WgcRuntimeAvailability,
    media_foundation_availability: MediaFoundationAvailability,
    recording_availability: RecordingRuntimeAvailability,
    audio: &AudioCapabilitySummary,
) -> [DoctorCheck; 4] {
    [
        doctor_check(
            DoctorCheckName::CaptureAvailability,
            capture_doctor_status(capture_availability),
            capture_doctor_detail(capture_availability),
        ),
        doctor_check(
            DoctorCheckName::EncoderAvailability,
            media_foundation_doctor_status(media_foundation_availability),
            media_foundation_doctor_detail(media_foundation_availability),
        ),
        doctor_check(
            DoctorCheckName::RecordingAvailability,
            recording_doctor_status(capture_availability, recording_availability),
            recording_doctor_detail(capture_availability, recording_availability),
        ),
        doctor_check(
            DoctorCheckName::AudioAvailability,
            audio_doctor_status(audio),
            audio_doctor_detail(audio),
        ),
    ]
}

const fn rollback_doctor_status(cache: NativeCacheHealth) -> DoctorStatus {
    if cache.rollback_protected {
        DoctorStatus::Pass
    } else {
        DoctorStatus::Fail
    }
}

const fn rollback_doctor_detail(status: CacheSchemaStatus) -> &'static str {
    match status {
        CacheSchemaStatus::Current => "native cache schema is current",
        CacheSchemaStatus::NeedsReset => "native cache is resettable",
        CacheSchemaStatus::RepairRequiredNewer { .. } => {
            "native cache schema is newer than this app"
        }
        CacheSchemaStatus::RepairRequiredInvalid => "native cache schema is invalid",
    }
}

const fn install_directory_acl_doctor_status(status: InstallDirectoryAclStatus) -> DoctorStatus {
    if install_directory_acl_repair_required(status) {
        DoctorStatus::Fail
    } else {
        DoctorStatus::Pass
    }
}

const fn install_directory_acl_doctor_detail(status: InstallDirectoryAclStatus) -> &'static str {
    match status {
        InstallDirectoryAclStatus::Protected => "install directory ACL is protected",
        InstallDirectoryAclStatus::BroadWriteAccess => {
            "install directory allows broad write access"
        }
        InstallDirectoryAclStatus::Unknown => "install directory ACL state is unknown",
        InstallDirectoryAclStatus::UnsupportedPlatform => {
            "install directory ACL check is unavailable on this platform"
        }
    }
}

fn install_repair_checks(report: &RepairRegistrationReport) -> Vec<DoctorCheck> {
    if report.issues.is_empty() {
        return vec![doctor_check(
            DoctorCheckName::NativeHostManifest,
            DoctorStatus::Pass,
            "native-host registration is healthy",
        )];
    }
    report
        .issues
        .iter()
        .map(|issue| {
            doctor_check(
                install_issue_check_name(issue.category),
                DoctorStatus::Fail,
                install_issue_detail(issue.category, issue.code),
            )
        })
        .collect()
}

const fn install_issue_check_name(category: &str) -> DoctorCheckName {
    match category.as_bytes() {
        b"binary-signature" => DoctorCheckName::BinarySignature,
        b"autostart" => DoctorCheckName::Autostart,
        _ => DoctorCheckName::NativeHostManifest,
    }
}

const fn install_issue_detail(category: &str, code: &str) -> &'static str {
    match (category.as_bytes(), code.as_bytes()) {
        (b"native-host-manifest", b"missing-manifest") => "native-host manifest is missing",
        (b"native-host-manifest", b"path-drift") => "native-host manifest path drift",
        (b"native-host-shadow-manifest", b"path-drift") => {
            "shadowing native-host manifest path drift"
        }
        (b"native-host-shadow-manifest", b"wrong-channel") => {
            "shadowing native-host manifest channel drift"
        }
        (b"binary-signature", b"required-signature-missing") => "binary signature is missing",
        (b"binary-signature", b"unknown-signature-state") => "binary signature state is unknown",
        (b"autostart", b"native-host-mode") => "autostart launches native-host mode",
        (b"autostart", b"invalid-command") => "autostart command is invalid",
        (b"autostart", b"unknown") => "autostart state is unknown",
        _ => "install registration requires repair",
    }
}

const fn capture_doctor_status(availability: WgcRuntimeAvailability) -> DoctorStatus {
    match availability {
        WgcRuntimeAvailability::Available => DoctorStatus::Pass,
        WgcRuntimeAvailability::UnsupportedPlatform | WgcRuntimeAvailability::ApiUnavailable => {
            DoctorStatus::Fail
        }
    }
}

const fn capture_doctor_detail(availability: WgcRuntimeAvailability) -> &'static str {
    match availability {
        WgcRuntimeAvailability::Available => "Windows Graphics Capture is available",
        WgcRuntimeAvailability::UnsupportedPlatform => {
            "Windows Graphics Capture is unavailable on this platform"
        }
        WgcRuntimeAvailability::ApiUnavailable => "Windows Graphics Capture API is unavailable",
    }
}

const fn media_foundation_doctor_status(availability: MediaFoundationAvailability) -> DoctorStatus {
    match availability {
        MediaFoundationAvailability::Available => DoctorStatus::Pass,
        MediaFoundationAvailability::UnsupportedPlatform
        | MediaFoundationAvailability::RuntimeUnavailable => DoctorStatus::Fail,
    }
}

const fn media_foundation_doctor_detail(availability: MediaFoundationAvailability) -> &'static str {
    match availability {
        MediaFoundationAvailability::Available => "Media Foundation runtime is available",
        MediaFoundationAvailability::UnsupportedPlatform => {
            "Media Foundation is unavailable on this platform"
        }
        MediaFoundationAvailability::RuntimeUnavailable => {
            "Media Foundation runtime is unavailable"
        }
    }
}

pub(crate) const fn recording_doctor_status(
    capture: WgcRuntimeAvailability,
    recording: RecordingRuntimeAvailability,
) -> DoctorStatus {
    if !matches!(capture, WgcRuntimeAvailability::Available) {
        return DoctorStatus::Fail;
    }
    match recording {
        RecordingRuntimeAvailability::Available => DoctorStatus::Pass,
        RecordingRuntimeAvailability::UnsupportedPlatform
        | RecordingRuntimeAvailability::EncoderUnavailable
        | RecordingRuntimeAvailability::RuntimeUnavailable => DoctorStatus::Fail,
    }
}

pub(crate) const fn recording_doctor_detail(
    capture: WgcRuntimeAvailability,
    recording: RecordingRuntimeAvailability,
) -> &'static str {
    if !matches!(capture, WgcRuntimeAvailability::Available) {
        return "recording runtime is unavailable because capture runtime is unavailable";
    }
    match recording {
        RecordingRuntimeAvailability::Available => "recording runtime is available",
        RecordingRuntimeAvailability::UnsupportedPlatform => {
            "recording runtime is unavailable on this platform"
        }
        RecordingRuntimeAvailability::EncoderUnavailable => {
            "recording runtime is unavailable because the encoder runtime is unavailable"
        }
        RecordingRuntimeAvailability::RuntimeUnavailable => {
            "recording runtime adapter is unavailable"
        }
    }
}

fn audio_doctor_status(audio: &AudioCapabilitySummary) -> DoctorStatus {
    if audio.supports_mixed_audio {
        DoctorStatus::Pass
    } else if audio.supports_microphone || audio.supports_system_audio {
        DoctorStatus::Warn
    } else {
        DoctorStatus::Fail
    }
}

fn audio_doctor_detail(audio: &AudioCapabilitySummary) -> &'static str {
    if audio.supports_mixed_audio {
        "microphone and system audio are available"
    } else if audio.supports_microphone {
        "microphone is available but system audio is unavailable"
    } else if audio.supports_system_audio {
        "system audio is available but microphone is unavailable"
    } else {
        "audio capture endpoints are unavailable"
    }
}
