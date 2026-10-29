use super::super::registration::{
    AutostartRegistrationFact, RegistrationRepairInputs, assess_registration_repair,
    current_repair_registration_report, repair_registration_report,
    repair_registration_report_for_inputs, startup_signature_policy,
};
use super::super::registry::{
    InstallChannel, ManifestError, NativeHostManifest, expected_host_name,
};
use super::super::repair::{
    AutostartState, RepairIssue, ReplacementDecision, ReplacementSafety, assess_install_repair,
};
use super::super::signature::{
    BinarySignatureReport, SignatureError, SignaturePolicy, SignatureProbeResult, SignatureStatus,
    evaluate_signature, signature_report_from_probe,
};

fn valid_signature() -> BinarySignatureReport {
    BinarySignatureReport {
        status: SignatureStatus::Valid,
        signer_subject: Some("Sniptale".to_owned()),
    }
}

#[test]
fn unsigned_signature_relaxation_requires_dev_channel_and_exact_dev_trust() {
    assert_eq!(
        startup_signature_policy(InstallChannel::Dev, true, false),
        SignaturePolicy::BestEffortDev
    );
    for (channel, trusted) in [
        (InstallChannel::Stable, true),
        (InstallChannel::Beta, true),
        (InstallChannel::Dev, false),
    ] {
        assert_eq!(
            startup_signature_policy(channel, trusted, false),
            SignaturePolicy::Required
        );
    }
}

#[test]
fn unsigned_alpha_startup_accepts_only_missing_dev_signature_and_preserves_host_trust() {
    let alpha_policy = startup_signature_policy(InstallChannel::Dev, false, true);
    for (status, accepted) in [
        (SignatureStatus::Missing, true),
        (SignatureStatus::Valid, true),
        (SignatureStatus::Invalid, false),
        (SignatureStatus::Unknown, false),
    ] {
        let report = BinarySignatureReport {
            status,
            signer_subject: None,
        };
        assert_eq!(evaluate_signature(&report, alpha_policy).is_ok(), accepted);
    }
    for channel in [InstallChannel::Stable, InstallChannel::Beta] {
        assert_eq!(
            startup_signature_policy(channel, false, true),
            SignaturePolicy::Required
        );
    }
    assert_eq!(
        startup_signature_policy(InstallChannel::Dev, false, false),
        SignaturePolicy::Required
    );
    let identity = crate::install::identity::InstallRuntimeIdentity::for_channel_and_origins(
        InstallChannel::Dev,
        [EXPECTED_ORIGIN],
    );
    let mut manifest = registration_manifest();
    manifest.host_name = expected_host_name(InstallChannel::Dev).to_owned();
    manifest.channel = InstallChannel::Dev;
    assert!(
        crate::install::registration_facts::trusted_registered_host_candidate(
            &manifest,
            EXPECTED_PATH,
            identity,
            SignatureStatus::Valid,
            false,
        )
    );
    assert!(
        !crate::install::registration_facts::trusted_registered_host_candidate(
            &manifest,
            EXPECTED_PATH,
            identity,
            SignatureStatus::Missing,
            false,
        )
    );
}

const EXPECTED_ORIGIN: &str = "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/";
const EXPECTED_PATH: &str =
    "C:\\Users\\Alice\\AppData\\Local\\Sniptale\\NativeAgent\\sniptale-native-agent.exe";

fn registration_manifest() -> NativeHostManifest {
    NativeHostManifest {
        host_name: expected_host_name(InstallChannel::Stable).to_owned(),
        manifest_version: "0.1.0".to_owned(),
        allowed_origins: vec![EXPECTED_ORIGIN.to_owned()],
        path: EXPECTED_PATH.to_owned(),
        channel: InstallChannel::Stable,
    }
}

fn registration_inputs<'a>(
    manifest: Result<&'a NativeHostManifest, ManifestError>,
    signature_report: &'a BinarySignatureReport,
    autostart: AutostartRegistrationFact<'a>,
) -> RegistrationRepairInputs<'a> {
    RegistrationRepairInputs {
        manifest,
        shadow_manifests: &[],
        expected_channel: InstallChannel::Stable,
        expected_executable_path: EXPECTED_PATH,
        expected_allowed_origins: &[EXPECTED_ORIGIN],
        signature_report,
        signature_policy: SignaturePolicy::Required,
        autostart,
        replacement_safety: ReplacementSafety::Idle,
    }
}

#[test]
fn production_signature_policy_fails_closed() {
    let missing = BinarySignatureReport {
        status: SignatureStatus::Missing,
        signer_subject: None,
    };
    assert_eq!(
        evaluate_signature(&missing, SignaturePolicy::Required),
        Err(SignatureError::RequiredSignatureMissing)
    );

    let invalid = BinarySignatureReport {
        status: SignatureStatus::Invalid,
        signer_subject: Some("Unknown".to_owned()),
    };
    assert_eq!(
        evaluate_signature(&invalid, SignaturePolicy::Required),
        Err(SignatureError::InvalidSignature)
    );
}

#[test]
fn dev_signature_policy_allows_missing_but_not_unknown_or_invalid() {
    let missing = BinarySignatureReport {
        status: SignatureStatus::Missing,
        signer_subject: None,
    };
    assert!(evaluate_signature(&missing, SignaturePolicy::BestEffortDev).is_ok());

    let unknown = BinarySignatureReport {
        status: SignatureStatus::Unknown,
        signer_subject: None,
    };
    assert_eq!(
        evaluate_signature(&unknown, SignaturePolicy::BestEffortDev),
        Err(SignatureError::UnknownSignatureState)
    );

    let invalid = BinarySignatureReport {
        status: SignatureStatus::Invalid,
        signer_subject: None,
    };
    assert_eq!(
        evaluate_signature(&invalid, SignaturePolicy::BestEffortDev),
        Err(SignatureError::InvalidSignature)
    );
}

#[test]
fn signature_probe_results_map_to_repair_signature_status() {
    assert_eq!(
        signature_report_from_probe(SignatureProbeResult::Valid).status,
        SignatureStatus::Valid
    );
    assert_eq!(
        signature_report_from_probe(SignatureProbeResult::Missing).status,
        SignatureStatus::Missing
    );
    assert_eq!(
        signature_report_from_probe(SignatureProbeResult::Invalid).status,
        SignatureStatus::Invalid
    );
    assert_eq!(
        signature_report_from_probe(SignatureProbeResult::Unknown).status,
        SignatureStatus::Unknown
    );
}

#[test]
fn repair_assessment_reports_manifest_signature_and_autostart_drift() {
    let unsigned = BinarySignatureReport {
        status: SignatureStatus::Missing,
        signer_subject: None,
    };
    let assessment = assess_install_repair(
        Err(ManifestError::PathDrift),
        &unsigned,
        SignaturePolicy::Required,
        AutostartState::NativeHostMode,
        ReplacementSafety::Idle,
    );

    assert!(assessment.repair_required);
    assert_eq!(
        assessment.replacement_decision,
        ReplacementDecision::Allowed
    );
    assert_eq!(
        assessment.issues,
        vec![
            RepairIssue::Manifest(ManifestError::PathDrift),
            RepairIssue::Signature(SignatureError::RequiredSignatureMissing),
            RepairIssue::AutostartWrongMode,
        ]
    );
}

#[test]
fn replacement_is_deferred_while_recording_or_transfer_is_active() {
    for safety in [
        ReplacementSafety::ActiveRecording,
        ReplacementSafety::ActiveTransfer,
        ReplacementSafety::Finalizing,
    ] {
        let assessment = assess_install_repair(
            Ok(()),
            &valid_signature(),
            SignaturePolicy::Required,
            AutostartState::EnabledResidentAgent,
            safety,
        );

        assert!(!assessment.repair_required);
        assert_eq!(
            assessment.replacement_decision,
            ReplacementDecision::DeferUntilIdle
        );
    }
}

#[test]
fn disabled_or_path_drifted_autostart_requires_repair() {
    for (state, issue) in [
        (AutostartState::Disabled, RepairIssue::AutostartDisabled),
        (AutostartState::PathDrift, RepairIssue::AutostartPathDrift),
        (
            AutostartState::InvalidCommand,
            RepairIssue::AutostartInvalidCommand,
        ),
        (AutostartState::Unknown, RepairIssue::AutostartUnknown),
    ] {
        let assessment = assess_install_repair(
            Ok(()),
            &valid_signature(),
            SignaturePolicy::Required,
            state,
            ReplacementSafety::Idle,
        );

        assert_eq!(assessment.issues, vec![issue]);
        assert!(assessment.repair_required);
    }
}

#[test]
fn missing_manifest_is_repair_required() {
    let assessment = assess_install_repair(
        Err(ManifestError::MissingManifest),
        &valid_signature(),
        SignaturePolicy::Required,
        AutostartState::EnabledResidentAgent,
        ReplacementSafety::Idle,
    );

    assert_eq!(
        assessment.issues,
        vec![RepairIssue::Manifest(ManifestError::MissingManifest)]
    );
    assert!(assessment.repair_required);
}

#[test]
fn registration_repair_composer_accepts_healthy_facts() {
    let manifest = registration_manifest();
    let command = format!("\"{EXPECTED_PATH}\"");
    let assessment = assess_registration_repair(registration_inputs(
        Ok(&manifest),
        &valid_signature(),
        AutostartRegistrationFact::Command(Some(&command)),
    ));

    assert!(!assessment.repair_required);
    assert_eq!(
        assessment.replacement_decision,
        ReplacementDecision::Allowed
    );
    assert!(assessment.issues.is_empty());
}

#[test]
fn registration_repair_composer_reports_all_fact_drift() {
    let mut manifest = registration_manifest();
    manifest.path = "C:\\Temp\\sniptale-native-agent.exe".to_owned();
    let unsigned = BinarySignatureReport {
        status: SignatureStatus::Missing,
        signer_subject: None,
    };
    let command = format!("\"{EXPECTED_PATH}\" --native-host");
    let assessment = assess_registration_repair(registration_inputs(
        Ok(&manifest),
        &unsigned,
        AutostartRegistrationFact::Command(Some(&command)),
    ));

    assert!(assessment.repair_required);
    assert_eq!(
        assessment.issues,
        vec![
            RepairIssue::Manifest(ManifestError::PathDrift),
            RepairIssue::Signature(SignatureError::RequiredSignatureMissing),
            RepairIssue::AutostartWrongMode,
        ]
    );
}

#[test]
fn registration_repair_composer_reports_stale_shadowing_manifests() {
    let manifest = registration_manifest();
    let mut shadow = registration_manifest();
    shadow.path = "C:\\Temp\\sniptale-native-agent.exe".to_owned();
    let command = format!("\"{EXPECTED_PATH}\"");
    let signature = valid_signature();
    let mut inputs = registration_inputs(
        Ok(&manifest),
        &signature,
        AutostartRegistrationFact::Command(Some(&command)),
    );
    let shadow_manifests = [Ok(&shadow), Err(ManifestError::MissingManifest)];
    inputs.shadow_manifests = &shadow_manifests;

    let assessment = assess_registration_repair(inputs);

    assert!(assessment.repair_required);
    assert_eq!(
        assessment.issues,
        vec![RepairIssue::ShadowManifest(ManifestError::PathDrift)]
    );
    let report = repair_registration_report(&assessment);
    assert_eq!(report.issues[0].category, "native-host-shadow-manifest");
    assert_eq!(report.issues[0].code, "path-drift");
}

#[test]
fn registration_report_for_inputs_preserves_missing_and_invalid_command_codes() {
    let command = format!("\"{EXPECTED_PATH}");
    let report = repair_registration_report_for_inputs(registration_inputs(
        Err(ManifestError::MissingManifest),
        &valid_signature(),
        AutostartRegistrationFact::Command(Some(&command)),
    ));

    assert_eq!(report.status, "repair-required");
    assert!(report.repair_required);
    assert_eq!(report.issues.len(), 2);
    assert_eq!(report.issues[0].category, "native-host-manifest");
    assert_eq!(report.issues[0].code, "missing-manifest");
    assert_eq!(report.issues[1].category, "autostart");
    assert_eq!(report.issues[1].code, "invalid-command");
}

#[test]
fn repair_registration_report_uses_redacted_issue_categories() {
    let unsigned = BinarySignatureReport {
        status: SignatureStatus::Unknown,
        signer_subject: None,
    };
    let assessment = assess_install_repair(
        Err(ManifestError::MissingManifest),
        &unsigned,
        SignaturePolicy::Required,
        AutostartState::Unknown,
        ReplacementSafety::ActiveTransfer,
    );
    let report = repair_registration_report(&assessment);

    assert_eq!(report.status, "repair-required");
    assert!(report.repair_required);
    assert_eq!(report.replacement_decision, "defer-until-idle");
    assert_eq!(report.issues.len(), 3);
    assert_eq!(report.issues[0].category, "native-host-manifest");
    assert_eq!(report.issues[0].code, "missing-manifest");
    assert_eq!(report.issues[1].category, "binary-signature");
    assert_eq!(report.issues[1].code, "unknown-signature-state");
    assert_eq!(report.issues[2].category, "autostart");
    assert_eq!(report.issues[2].code, "unknown");
}

#[test]
fn current_repair_registration_report_fails_closed_when_runtime_facts_are_unhealthy() {
    let report = current_repair_registration_report();

    assert_eq!(report.status, "repair-required");
    assert!(report.repair_required);
    assert!(
        report
            .issues
            .iter()
            .any(|issue| issue.code == "missing-manifest")
    );
}
