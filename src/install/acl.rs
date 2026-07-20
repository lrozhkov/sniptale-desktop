//! Install directory ACL trust facts.

use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InstallDirectoryAclStatus {
    #[cfg_attr(not(windows), allow(dead_code))]
    Protected,
    #[cfg_attr(not(windows), allow(dead_code))]
    BroadWriteAccess,
    Unknown,
    #[cfg_attr(windows, allow(dead_code))]
    UnsupportedPlatform,
}

pub(crate) fn current_install_directory_acl_status() -> InstallDirectoryAclStatus {
    let Ok(executable_path) = std::env::current_exe() else {
        return InstallDirectoryAclStatus::Unknown;
    };
    let Some(install_directory) = executable_path.parent() else {
        return InstallDirectoryAclStatus::Unknown;
    };
    install_directory_acl_status(install_directory)
}

pub(super) fn install_directory_acl_status(path: &Path) -> InstallDirectoryAclStatus {
    platform_install_directory_acl_status(path)
}

pub(crate) const fn install_directory_acl_repair_required(
    status: InstallDirectoryAclStatus,
) -> bool {
    match status {
        InstallDirectoryAclStatus::Protected => false,
        InstallDirectoryAclStatus::BroadWriteAccess
        | InstallDirectoryAclStatus::Unknown
        | InstallDirectoryAclStatus::UnsupportedPlatform => true,
    }
}

#[cfg(windows)]
fn platform_install_directory_acl_status(path: &Path) -> InstallDirectoryAclStatus {
    super::windows::acl::install_directory_acl_status(path)
}

#[cfg(not(windows))]
fn platform_install_directory_acl_status(_path: &Path) -> InstallDirectoryAclStatus {
    InstallDirectoryAclStatus::UnsupportedPlatform
}
