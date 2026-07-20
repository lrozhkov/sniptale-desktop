//! Repair-required diagnostics owner.

use super::registry::ManifestError;
use super::signature::{
    BinarySignatureReport, SignatureError, SignaturePolicy, evaluate_signature,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AutostartState {
    EnabledResidentAgent,
    Disabled,
    NativeHostMode,
    PathDrift,
    InvalidCommand,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReplacementSafety {
    Idle,
    #[cfg(test)]
    ActiveRecording,
    #[cfg(test)]
    ActiveTransfer,
    #[cfg(test)]
    Finalizing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReplacementDecision {
    Allowed,
    #[cfg(test)]
    DeferUntilIdle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RepairIssue {
    Manifest(ManifestError),
    ShadowManifest(ManifestError),
    Signature(SignatureError),
    AutostartDisabled,
    AutostartWrongMode,
    AutostartPathDrift,
    AutostartInvalidCommand,
    AutostartUnknown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstallRepairAssessment {
    pub(crate) repair_required: bool,
    pub(crate) replacement_decision: ReplacementDecision,
    pub(crate) issues: Vec<RepairIssue>,
}

pub(crate) fn assess_install_repair(
    manifest_result: Result<(), ManifestError>,
    signature_report: &BinarySignatureReport,
    signature_policy: SignaturePolicy,
    autostart_state: AutostartState,
    replacement_safety: ReplacementSafety,
) -> InstallRepairAssessment {
    let mut issues = Vec::new();
    if let Err(error) = manifest_result {
        issues.push(RepairIssue::Manifest(error));
    }
    if let Err(error) = evaluate_signature(signature_report, signature_policy) {
        issues.push(RepairIssue::Signature(error));
    }
    if let Some(issue) = autostart_issue(autostart_state) {
        issues.push(issue);
    }

    InstallRepairAssessment {
        repair_required: !issues.is_empty(),
        replacement_decision: replacement_decision(replacement_safety),
        issues,
    }
}

fn autostart_issue(state: AutostartState) -> Option<RepairIssue> {
    match state {
        AutostartState::EnabledResidentAgent => None,
        AutostartState::Disabled => Some(RepairIssue::AutostartDisabled),
        AutostartState::NativeHostMode => Some(RepairIssue::AutostartWrongMode),
        AutostartState::PathDrift => Some(RepairIssue::AutostartPathDrift),
        AutostartState::InvalidCommand => Some(RepairIssue::AutostartInvalidCommand),
        AutostartState::Unknown => Some(RepairIssue::AutostartUnknown),
    }
}

const fn replacement_decision(safety: ReplacementSafety) -> ReplacementDecision {
    match safety {
        ReplacementSafety::Idle => ReplacementDecision::Allowed,
        #[cfg(test)]
        ReplacementSafety::ActiveRecording
        | ReplacementSafety::ActiveTransfer
        | ReplacementSafety::Finalizing => ReplacementDecision::DeferUntilIdle,
    }
}
