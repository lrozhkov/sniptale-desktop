//! Runtime native-host install identity startup gate.

#[cfg(all(windows, not(test)))]
use super::registration_facts::current_registration_facts;
use super::{
    registration_facts::CurrentRegistrationFacts,
    registry::{ManifestError, validate_manifest},
};

#[cfg(all(windows, not(test)))]
pub(crate) fn verify_current_runtime_manifest_identity() -> Result<(), ManifestError> {
    verify_runtime_manifest_identity(&current_registration_facts())
}

pub(crate) fn verify_runtime_manifest_identity(
    facts: &CurrentRegistrationFacts,
) -> Result<(), ManifestError> {
    validate_manifest(
        facts.manifest.as_ref().map_err(|error| *error)?,
        facts.expected_channel,
        &facts.expected_executable_path,
        &facts.expected_allowed_origins,
    )?;
    for manifest in &facts.shadow_manifests {
        validate_manifest(
            manifest.as_ref().map_err(|error| *error)?,
            facts.expected_channel,
            &facts.expected_executable_path,
            &facts.expected_allowed_origins,
        )?;
    }
    Ok(())
}
