//! Windows named-pipe security attributes adapter.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::{ffi::OsStr, io, mem, os::windows::ffi::OsStrExt};

use windows::{
    Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::{
            Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION},
            PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES,
        },
    },
    core::{BOOL, PCWSTR},
};

use super::security::{PipeSecurityDescriptorError, current_user_pipe_sddl};

#[derive(Debug)]
pub(crate) struct WindowsPipeSecurityAttributes {
    descriptor: PSECURITY_DESCRIPTOR,
    attributes: SECURITY_ATTRIBUTES,
}

impl WindowsPipeSecurityAttributes {
    pub(crate) fn new(current_user_sid: &str) -> io::Result<Self> {
        let sddl = current_user_pipe_sddl(current_user_sid).map_err(pipe_security_error)?;
        let descriptor = security_descriptor_from_sddl(&sddl)?;
        let attributes = SECURITY_ATTRIBUTES {
            nLength: u32::try_from(mem::size_of::<SECURITY_ATTRIBUTES>())
                .map_err(|_| io::Error::other("security attributes size exceeds u32"))?,
            lpSecurityDescriptor: descriptor.0,
            bInheritHandle: BOOL(0),
        };
        Ok(Self {
            descriptor,
            attributes,
        })
    }

    pub(crate) const fn as_ptr(&self) -> *const SECURITY_ATTRIBUTES {
        &self.attributes
    }
}

impl Drop for WindowsPipeSecurityAttributes {
    fn drop(&mut self) {
        if self.descriptor.is_invalid() {
            return;
        }
        // SAFETY: ConvertStringSecurityDescriptorToSecurityDescriptorW returned this LocalAlloc
        // security descriptor; it is freed exactly once after CreateNamedPipeW has returned.
        let _ = unsafe { LocalFree(Some(HLOCAL(self.descriptor.0))) };
        self.descriptor = PSECURITY_DESCRIPTOR::default();
    }
}

fn security_descriptor_from_sddl(sddl: &str) -> io::Result<PSECURITY_DESCRIPTOR> {
    let sddl = wide_null(OsStr::new(sddl));
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: `sddl` is a NUL-terminated SDDL string built by the IPC owner; the returned
    // LocalAlloc descriptor is wrapped by WindowsPipeSecurityAttributes for LocalFree ownership.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(sddl.as_ptr()),
            SDDL_REVISION,
            &mut descriptor,
            None,
        )
    }
    .map_err(|error| io::Error::from_raw_os_error(error.code().0))?;
    if descriptor.is_invalid() {
        Err(io::Error::other(
            "security descriptor conversion returned null",
        ))
    } else {
        Ok(descriptor)
    }
}

fn pipe_security_error(error: PipeSecurityDescriptorError) -> io::Error {
    match error {
        PipeSecurityDescriptorError::InvalidUserSid => {
            io::Error::new(io::ErrorKind::InvalidInput, "invalid current-user SID")
        }
    }
}

fn wide_null(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain([0]).collect()
}
