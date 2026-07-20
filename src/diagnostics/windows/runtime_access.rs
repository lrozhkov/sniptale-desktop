//! Windows runtime data directory ACL adapter.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::{ffi::OsStr, os::windows::ffi::OsStrExt, path::Path};

use windows::{
    Win32::{
        Foundation::{ERROR_SUCCESS, GENERIC_ALL, GENERIC_WRITE, HLOCAL, LocalFree},
        Security::{
            ACL,
            Authorization::{
                BuildTrusteeWithSidW, ConvertStringSidToSidW, GetEffectiveRightsFromAclW,
                GetNamedSecurityInfoW, SE_FILE_OBJECT, TRUSTEE_W,
            },
            DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
        },
        Storage::FileSystem::{
            DELETE, FILE_DELETE_CHILD, FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_READ_EA,
            FILE_WRITE_ATTRIBUTES, FILE_WRITE_EA, WRITE_DAC, WRITE_OWNER,
        },
    },
    core::PCWSTR,
};

use super::super::runtime_access::RuntimeDirectoryAccessStatus;

const BROAD_ACCESS_SIDS: [&str; 3] = [
    "S-1-1-0",      // Everyone
    "S-1-5-11",     // Authenticated Users
    "S-1-5-32-545", // BUILTIN\Users
];

pub(crate) fn runtime_directory_access_status(path: &Path) -> RuntimeDirectoryAccessStatus {
    if path.as_os_str().is_empty() {
        return RuntimeDirectoryAccessStatus::Unknown;
    }
    match query_directory_dacl(path) {
        Ok(dacl) => access_status_from_dacl(dacl.dacl),
        Err(()) => RuntimeDirectoryAccessStatus::Unknown,
    }
}

fn access_status_from_dacl(dacl: *mut ACL) -> RuntimeDirectoryAccessStatus {
    if dacl.is_null() {
        return RuntimeDirectoryAccessStatus::BroadWriteAccess;
    }

    let mut broad_read = false;
    for sid in BROAD_ACCESS_SIDS {
        let Ok(sid) = LocalSid::from_string(sid) else {
            return RuntimeDirectoryAccessStatus::Unknown;
        };
        let Ok(rights) = effective_rights_for_sid(dacl, sid.value) else {
            return RuntimeDirectoryAccessStatus::Unknown;
        };
        if has_broad_write_access(rights) {
            return RuntimeDirectoryAccessStatus::BroadWriteAccess;
        }
        broad_read |= has_broad_read_access(rights);
    }

    if broad_read {
        RuntimeDirectoryAccessStatus::BroadReadAccess
    } else {
        RuntimeDirectoryAccessStatus::Protected
    }
}

fn query_directory_dacl(path: &Path) -> Result<DirectoryDacl, ()> {
    let path = wide_null(path.as_os_str());
    let mut dacl: *mut ACL = std::ptr::null_mut();
    let mut security_descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: `path` is a NUL-terminated Windows path buffer; output pointers are valid stack
    // slots. The returned security descriptor owns the DACL memory and is wrapped for LocalFree.
    let status = unsafe {
        GetNamedSecurityInfoW(
            PCWSTR(path.as_ptr()),
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            None,
            None,
            Some(&mut dacl),
            None,
            &mut security_descriptor,
        )
    };
    if status != ERROR_SUCCESS || security_descriptor.is_invalid() {
        return Err(());
    }
    Ok(DirectoryDacl {
        dacl,
        security_descriptor,
    })
}

fn effective_rights_for_sid(dacl: *const ACL, sid: PSID) -> Result<u32, ()> {
    let mut trustee = TRUSTEE_W::default();
    // SAFETY: `sid` is a valid LocalAlloc SID owned by LocalSid and lives through the API call.
    unsafe { BuildTrusteeWithSidW(&mut trustee, Some(sid)) };
    let mut rights = 0_u32;
    // SAFETY: `dacl` is owned by the security descriptor returned by GetNamedSecurityInfoW; trustee
    // points to a valid SID-backed TRUSTEE_W for the duration of this call.
    let status = unsafe { GetEffectiveRightsFromAclW(dacl, &trustee, &mut rights) };
    if status == ERROR_SUCCESS {
        Ok(rights)
    } else {
        Err(())
    }
}

fn has_broad_write_access(rights: u32) -> bool {
    // Directory list/read-data bits overlap file create/write-data bits. Keep this mask to rights
    // that distinguish mutation from read-only traversal after GetEffectiveRightsFromAclW expands
    // the ACE.
    let write_mask = GENERIC_WRITE.0
        | GENERIC_ALL.0
        | FILE_WRITE_EA.0
        | FILE_WRITE_ATTRIBUTES.0
        | FILE_DELETE_CHILD.0
        | WRITE_DAC.0
        | WRITE_OWNER.0
        | DELETE.0;
    rights & write_mask != 0
}

fn has_broad_read_access(rights: u32) -> bool {
    let read_mask = FILE_LIST_DIRECTORY.0 | FILE_READ_EA.0 | FILE_READ_ATTRIBUTES.0;
    rights & read_mask != 0
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain([0]).collect()
}

#[derive(Debug)]
struct DirectoryDacl {
    dacl: *mut ACL,
    security_descriptor: PSECURITY_DESCRIPTOR,
}

impl Drop for DirectoryDacl {
    fn drop(&mut self) {
        if self.security_descriptor.is_invalid() {
            return;
        }
        // SAFETY: GetNamedSecurityInfoW returned this LocalAlloc security descriptor; it is freed
        // exactly once by this owner, after all DACL access is complete.
        let _ = unsafe { LocalFree(Some(HLOCAL(self.security_descriptor.0))) };
        self.security_descriptor = PSECURITY_DESCRIPTOR::default();
    }
}

#[derive(Debug)]
struct LocalSid {
    value: PSID,
}

impl LocalSid {
    fn from_string(value: &str) -> Result<Self, ()> {
        let value = wide_null(OsStr::new(value));
        let mut sid = PSID::default();
        // SAFETY: `value` is a NUL-terminated SID string; the returned LocalAlloc SID is wrapped
        // immediately for LocalFree ownership.
        unsafe { ConvertStringSidToSidW(PCWSTR(value.as_ptr()), &mut sid) }.map_err(|_| ())?;
        if sid.is_invalid() {
            Err(())
        } else {
            Ok(Self { value: sid })
        }
    }
}

impl Drop for LocalSid {
    fn drop(&mut self) {
        if self.value.is_invalid() {
            return;
        }
        // SAFETY: ConvertStringSidToSidW returned this LocalAlloc SID; it is freed exactly once by
        // this owner, after all trustee/effective-rights calls using it are complete.
        let _ = unsafe { LocalFree(Some(HLOCAL(self.value.0))) };
        self.value = PSID::default();
    }
}
