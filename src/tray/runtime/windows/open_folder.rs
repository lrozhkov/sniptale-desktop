//! Typed Manual-destination shell opener.
#![allow(unsafe_code)]

use std::{ffi::OsStr, os::windows::ffi::OsStrExt, path::Path};

use windows::{
    Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL},
    core::PCWSTR,
};

use super::super::source::TrayRuntimeError;

pub(super) fn open(destination: &Path) -> Result<(), TrayRuntimeError> {
    let verb = wide_null("open");
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain([0])
        .collect::<Vec<_>>();
    // SAFETY: the destination is revalidated by the tray dispatcher and is only supplied to the
    // synchronous shell invocation; both UTF-16 buffers are NUL-terminated and remain alive
    // through the call. This adapter does not log the destination or accept shell arguments.
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(verb.as_ptr()),
            PCWSTR(destination.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        )
    };
    if (result.0 as usize) <= 32 {
        Err(TrayRuntimeError::RenderFailed)
    } else {
        Ok(())
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain([0]).collect()
}
