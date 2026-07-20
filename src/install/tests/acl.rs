use super::super::{InstallDirectoryAclStatus, acl::install_directory_acl_repair_required};

#[test]
fn acl_policy_fails_closed_except_protected_install_directory() {
    assert!(!install_directory_acl_repair_required(
        InstallDirectoryAclStatus::Protected
    ));
    for status in [
        InstallDirectoryAclStatus::BroadWriteAccess,
        InstallDirectoryAclStatus::Unknown,
        InstallDirectoryAclStatus::UnsupportedPlatform,
    ] {
        assert!(install_directory_acl_repair_required(status));
    }
}

#[test]
fn non_windows_acl_status_is_platform_fact_not_core_assumption() {
    #[cfg(not(windows))]
    assert_eq!(
        super::super::acl::install_directory_acl_status(std::path::Path::new("/opt/sniptale")),
        InstallDirectoryAclStatus::UnsupportedPlatform
    );
}
