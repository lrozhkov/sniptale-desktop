//! Install runtime health projection.

#[cfg(any(windows, test))]
use super::registry::{ManifestError, validate_manifest};
use super::{
    autostart::evaluate_autostart_command,
    registration_facts::{
        CurrentRegistrationFacts, RuntimeAutostartFact, current_registration_facts,
    },
    repair::AutostartState,
    signature::SignatureStatus,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InstallRuntimeHealth {
    pub(crate) signed_binary: bool,
    pub(crate) package_integrity: InstallPackageIntegrity,
    pub(crate) autostart_enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InstallPackageIntegrity {
    #[cfg(any(windows, test))]
    Valid,
    #[cfg(any(windows, test))]
    Invalid,
    Unknown,
}

pub(crate) fn current_install_runtime_health() -> InstallRuntimeHealth {
    let facts = current_registration_facts();
    install_runtime_health_from_facts(&facts)
}

pub(super) fn install_runtime_health_from_facts(
    facts: &CurrentRegistrationFacts,
) -> InstallRuntimeHealth {
    InstallRuntimeHealth {
        signed_binary: signature_is_valid(facts.signature_report.status),
        package_integrity: package_integrity_from_facts(facts),
        autostart_enabled: autostart_enabled(facts),
    }
}

#[cfg(any(windows, test))]
const fn signature_is_valid(status: SignatureStatus) -> bool {
    matches!(status, SignatureStatus::Valid)
}

#[cfg(not(any(windows, test)))]
const fn signature_is_valid(status: SignatureStatus) -> bool {
    match status {
        SignatureStatus::Unknown => false,
    }
}

fn package_integrity_from_facts(facts: &CurrentRegistrationFacts) -> InstallPackageIntegrity {
    match facts.signature_report.status {
        SignatureStatus::Unknown => InstallPackageIntegrity::Unknown,
        #[cfg(any(windows, test))]
        SignatureStatus::Missing | SignatureStatus::Invalid => InstallPackageIntegrity::Invalid,
        #[cfg(any(windows, test))]
        SignatureStatus::Valid => {
            if manifest_is_valid(facts) && shadow_manifests_are_valid(facts) {
                InstallPackageIntegrity::Valid
            } else {
                InstallPackageIntegrity::Invalid
            }
        }
    }
}

#[cfg(any(windows, test))]
fn manifest_is_valid(facts: &CurrentRegistrationFacts) -> bool {
    facts.manifest.as_ref().is_ok_and(|manifest| {
        validate_manifest(
            manifest,
            facts.expected_channel,
            &facts.expected_executable_path,
            &facts.expected_allowed_origins,
        )
        .is_ok()
    })
}

#[cfg(any(windows, test))]
fn shadow_manifests_are_valid(facts: &CurrentRegistrationFacts) -> bool {
    facts
        .shadow_manifests
        .iter()
        .all(|manifest| match manifest {
            Ok(manifest) => validate_manifest(
                manifest,
                facts.expected_channel,
                &facts.expected_executable_path,
                &facts.expected_allowed_origins,
            )
            .is_ok(),
            Err(ManifestError::MissingManifest) => true,
            Err(_) => false,
        })
}

fn autostart_enabled(facts: &CurrentRegistrationFacts) -> bool {
    match &facts.autostart_command {
        RuntimeAutostartFact::Command(command) => {
            evaluate_autostart_command(command.as_deref(), &facts.expected_executable_path)
                == AutostartState::EnabledResidentAgent
        }
        RuntimeAutostartFact::Unknown => false,
    }
}
