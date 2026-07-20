//! Runtime registration repair report owner.

#[cfg(windows)]
use std::path::PathBuf;

use serde::Serialize;

use super::{
    autostart::evaluate_autostart_command,
    registration_facts::{RuntimeAutostartFact, current_registration_facts},
    registry::{InstallChannel, ManifestError, NativeHostManifest, validate_manifest},
    repair::{
        AutostartState, InstallRepairAssessment, RepairIssue, ReplacementDecision,
        ReplacementSafety, assess_install_repair,
    },
    signature::{BinarySignatureReport, SignaturePolicy},
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RepairRegistrationReport {
    pub(crate) status: &'static str,
    pub(crate) repair_required: bool,
    pub(crate) replacement_decision: &'static str,
    pub(crate) issues: Vec<RepairRegistrationIssue>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RepairRegistrationIssue {
    pub(crate) category: &'static str,
    pub(crate) code: &'static str,
    pub(crate) repair_required: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct RegistrationRepairInputs<'a> {
    pub(crate) manifest: Result<&'a NativeHostManifest, ManifestError>,
    pub(crate) shadow_manifests: &'a [Result<&'a NativeHostManifest, ManifestError>],
    pub(crate) expected_channel: InstallChannel,
    pub(crate) expected_executable_path: &'a str,
    pub(crate) expected_allowed_origins: &'a [&'a str],
    pub(crate) signature_report: &'a BinarySignatureReport,
    pub(crate) signature_policy: SignaturePolicy,
    pub(crate) autostart: AutostartRegistrationFact<'a>,
    pub(crate) replacement_safety: ReplacementSafety,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AutostartRegistrationFact<'a> {
    #[allow(dead_code)]
    Command(Option<&'a str>),
    Unknown,
}

pub(crate) fn current_repair_registration_report() -> RepairRegistrationReport {
    let facts = current_registration_facts();
    let manifest = facts.manifest.as_ref().map_err(|error| *error);
    let shadow_manifests: Vec<Result<&NativeHostManifest, ManifestError>> = facts
        .shadow_manifests
        .iter()
        .map(|manifest| manifest.as_ref().map_err(|error| *error))
        .collect();
    let autostart = match &facts.autostart_command {
        RuntimeAutostartFact::Command(command) => {
            AutostartRegistrationFact::Command(command.as_deref())
        }
        RuntimeAutostartFact::Unknown => AutostartRegistrationFact::Unknown,
    };
    let inputs = RegistrationRepairInputs {
        manifest,
        shadow_manifests: &shadow_manifests,
        expected_channel: facts.expected_channel,
        expected_executable_path: &facts.expected_executable_path,
        expected_allowed_origins: &facts.expected_allowed_origins,
        signature_report: &facts.signature_report,
        signature_policy: current_signature_policy(facts.expected_channel),
        autostart,
        replacement_safety: ReplacementSafety::Idle,
    };
    repair_registration_report_for_inputs(inputs)
}

#[cfg(windows)]
fn current_signature_policy(channel: InstallChannel) -> SignaturePolicy {
    signature_policy_for_dev_trust(
        channel,
        super::dev_local_trust::dev_local_browser_trust_enabled(),
    )
}

#[cfg(not(windows))]
const fn current_signature_policy(_channel: InstallChannel) -> SignaturePolicy {
    SignaturePolicy::Required
}

#[cfg(any(windows, test))]
pub(super) const fn signature_policy_for_dev_trust(
    channel: InstallChannel,
    dev_local_trust: bool,
) -> SignaturePolicy {
    if matches!(channel, InstallChannel::Dev) && dev_local_trust {
        SignaturePolicy::BestEffortDev
    } else {
        SignaturePolicy::Required
    }
}

#[cfg(windows)]
pub(crate) fn current_program_files_roots() -> Vec<PathBuf> {
    super::windows::program_files::current_program_files_roots()
}

pub(crate) fn repair_registration_report_for_inputs(
    inputs: RegistrationRepairInputs<'_>,
) -> RepairRegistrationReport {
    let assessment = assess_registration_repair(inputs);
    repair_registration_report(&assessment)
}

pub(crate) fn assess_registration_repair(
    inputs: RegistrationRepairInputs<'_>,
) -> InstallRepairAssessment {
    let manifest_result = inputs.manifest.and_then(|manifest| {
        validate_manifest(
            manifest,
            inputs.expected_channel,
            inputs.expected_executable_path,
            inputs.expected_allowed_origins,
        )
    });
    let autostart_state = match inputs.autostart {
        AutostartRegistrationFact::Command(command) => {
            evaluate_autostart_command(command, inputs.expected_executable_path)
        }
        AutostartRegistrationFact::Unknown => AutostartState::Unknown,
    };
    let mut assessment = assess_install_repair(
        manifest_result,
        inputs.signature_report,
        inputs.signature_policy,
        autostart_state,
        inputs.replacement_safety,
    );
    assessment.issues.extend(shadow_manifest_issues(inputs));
    assessment.repair_required = !assessment.issues.is_empty();
    assessment
}

pub(crate) fn repair_registration_report(
    assessment: &InstallRepairAssessment,
) -> RepairRegistrationReport {
    RepairRegistrationReport {
        status: if assessment.repair_required {
            "repair-required"
        } else {
            "healthy"
        },
        repair_required: assessment.repair_required,
        replacement_decision: replacement_decision_code(assessment.replacement_decision),
        issues: assessment
            .issues
            .iter()
            .copied()
            .map(issue_report)
            .collect(),
    }
}

fn issue_report(issue: RepairIssue) -> RepairRegistrationIssue {
    let (category, code) = issue_code(issue);
    RepairRegistrationIssue {
        category,
        code,
        repair_required: true,
    }
}

fn shadow_manifest_issues(inputs: RegistrationRepairInputs<'_>) -> Vec<RepairIssue> {
    inputs
        .shadow_manifests
        .iter()
        .filter_map(|manifest| match manifest {
            Ok(manifest) => validate_manifest(
                manifest,
                inputs.expected_channel,
                inputs.expected_executable_path,
                inputs.expected_allowed_origins,
            )
            .err()
            .map(RepairIssue::ShadowManifest),
            Err(ManifestError::MissingManifest) => None,
            Err(error) => Some(RepairIssue::ShadowManifest(*error)),
        })
        .collect()
}

const fn issue_code(issue: RepairIssue) -> (&'static str, &'static str) {
    match issue {
        RepairIssue::Manifest(error) => ("native-host-manifest", manifest_error_code(error)),
        RepairIssue::ShadowManifest(error) => {
            ("native-host-shadow-manifest", manifest_error_code(error))
        }
        RepairIssue::Signature(error) => ("binary-signature", signature_error_code(error)),
        RepairIssue::AutostartDisabled => ("autostart", "disabled"),
        RepairIssue::AutostartWrongMode => ("autostart", "native-host-mode"),
        RepairIssue::AutostartPathDrift => ("autostart", "path-drift"),
        RepairIssue::AutostartInvalidCommand => ("autostart", "invalid-command"),
        RepairIssue::AutostartUnknown => ("autostart", "unknown"),
    }
}

const fn manifest_error_code(error: ManifestError) -> &'static str {
    match error {
        ManifestError::MissingManifest => "missing-manifest",
        ManifestError::UnreadableManifest => "unreadable-manifest",
        ManifestError::OversizeManifest => "oversize-manifest",
        ManifestError::InvalidJson => "invalid-json",
        ManifestError::UnsupportedManifestType => "unsupported-manifest-type",
        ManifestError::ManifestVersionDrift => "manifest-version-drift",
        ManifestError::InvalidHostName => "invalid-host-name",
        ManifestError::InvalidAllowedOrigin => "invalid-allowed-origin",
        ManifestError::MissingAllowedOrigin => "missing-allowed-origin",
        ManifestError::UnexpectedAllowedOrigin => "unexpected-allowed-origin",
        ManifestError::WrongChannel => "wrong-channel",
        ManifestError::PathDrift => "path-drift",
    }
}

const fn signature_error_code(error: super::signature::SignatureError) -> &'static str {
    match error {
        #[cfg(any(windows, test))]
        super::signature::SignatureError::RequiredSignatureMissing => "required-signature-missing",
        #[cfg(any(windows, test))]
        super::signature::SignatureError::InvalidSignature => "invalid-signature",
        super::signature::SignatureError::UnknownSignatureState => "unknown-signature-state",
    }
}

const fn replacement_decision_code(decision: ReplacementDecision) -> &'static str {
    match decision {
        ReplacementDecision::Allowed => "allowed",
        #[cfg(test)]
        ReplacementDecision::DeferUntilIdle => "defer-until-idle",
    }
}
