//! Native Messaging manifests, autostart, signature checks, and repair diagnostics.

mod acl;
mod autostart;
#[cfg(any(windows, test))]
mod dev_local_trust;
mod health;
mod identity;
#[cfg(test)]
mod manifest_writer;
#[cfg(any(windows, test))]
mod pipe_secret;
#[cfg(any(windows, test))]
mod pipe_secret_store;
mod registration;
mod registration_facts;
mod registry;
mod repair;
#[cfg(any(windows, test))]
mod runtime_identity_gate;
mod signature;
#[cfg(test)]
mod tests;
#[cfg(windows)]
mod windows;

pub(crate) use acl::{
    InstallDirectoryAclStatus, current_install_directory_acl_status,
    install_directory_acl_repair_required,
};
#[cfg(windows)]
pub(crate) use dev_local_trust::{
    current_user_local_app_data_roots, dev_local_browser_trust_enabled,
};
pub(crate) use health::{
    InstallPackageIntegrity, InstallRuntimeHealth, current_install_runtime_health,
};
pub(crate) use identity::expected_install_identity;
#[cfg(windows)]
pub(crate) use pipe_secret_store::initialize_current_user_pipe_secret;
#[cfg(windows)]
pub(crate) use pipe_secret_store::load_current_user_pipe_secret;
#[cfg(windows)]
pub(crate) use registration::{
    RepairRegistrationReport, current_program_files_roots, current_repair_registration_report,
};
#[cfg(not(windows))]
pub(crate) use registration::{RepairRegistrationReport, current_repair_registration_report};
#[cfg(windows)]
pub(crate) use registration_facts::trusted_native_host_executable_path;
#[cfg(all(windows, not(test)))]
pub(crate) use registry::ManifestError;
pub(crate) use registry::{
    INSTALLER_VERSION, InstallChannel, NATIVE_HOST_MANIFEST_VERSION, allowed_extension_origin,
};
#[cfg(all(windows, not(test)))]
pub(crate) use runtime_identity_gate::verify_current_runtime_manifest_identity;
