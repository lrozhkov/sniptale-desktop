//! Windows current-user SID adapter.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::{ffi::c_void, mem};

use windows::{
    Win32::{
        Foundation::{CloseHandle, HANDLE, HLOCAL, LocalFree},
        Security::{
            Authorization::ConvertSidToStringSidW, GetTokenInformation, TOKEN_QUERY, TOKEN_USER,
            TokenUser,
        },
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    },
    core::PWSTR,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowsUserError {
    OpenTokenFailed,
    TokenQueryFailed,
    TokenUserMissing,
    SidConversionFailed,
    InvalidSidString,
}

pub(crate) fn current_user_sid() -> Result<String, WindowsUserError> {
    let token = open_current_process_token()?;
    token_user_sid(token.handle)
}

fn open_current_process_token() -> Result<TokenHandle, WindowsUserError> {
    let mut handle = HANDLE::default();
    // SAFETY: GetCurrentProcess returns the current process pseudo-handle; OpenProcessToken writes
    // one owned token handle into `handle`, which is wrapped immediately for CloseHandle ownership.
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) }
        .map_err(|_| WindowsUserError::OpenTokenFailed)?;
    TokenHandle::new(handle)
}

fn token_user_sid(token: HANDLE) -> Result<String, WindowsUserError> {
    let mut required = 0_u32;
    // SAFETY: This first call intentionally passes no output buffer to obtain the required size.
    let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &mut required) };
    if required == 0 {
        return Err(WindowsUserError::TokenQueryFailed);
    }

    let mut buffer = vec![0_u8; required as usize];
    // SAFETY: `buffer` is allocated with the size requested by GetTokenInformation; the API writes
    // a TOKEN_USER record into the buffer for the lifetime of this function.
    unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            Some(buffer.as_mut_ptr().cast::<c_void>()),
            required,
            &mut required,
        )
    }
    .map_err(|_| WindowsUserError::TokenQueryFailed)?;

    if buffer.len() < mem::size_of::<TOKEN_USER>() {
        return Err(WindowsUserError::TokenUserMissing);
    }
    // SAFETY: GetTokenInformation succeeded for TokenUser and wrote a TOKEN_USER at buffer start.
    // The SID pointer remains valid while `buffer` is alive in this function.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    sid_to_string(user.User.Sid)
}

fn sid_to_string(sid: windows::Win32::Security::PSID) -> Result<String, WindowsUserError> {
    if sid.is_invalid() {
        return Err(WindowsUserError::TokenUserMissing);
    }
    let mut sid_string = PWSTR::null();
    // SAFETY: `sid` is the PSID returned by a successful TokenUser query; the returned string is a
    // LocalAlloc buffer and is wrapped immediately for LocalFree ownership.
    unsafe { ConvertSidToStringSidW(sid, &mut sid_string) }
        .map_err(|_| WindowsUserError::SidConversionFailed)?;
    let sid_string = LocalString::new(sid_string)?;
    sid_string.to_string()
}

#[derive(Debug)]
struct TokenHandle {
    handle: HANDLE,
}

impl TokenHandle {
    fn new(handle: HANDLE) -> Result<Self, WindowsUserError> {
        if handle.is_invalid() {
            Err(WindowsUserError::OpenTokenFailed)
        } else {
            Ok(Self { handle })
        }
    }
}

impl Drop for TokenHandle {
    fn drop(&mut self) {
        if !self.handle.is_invalid() {
            // SAFETY: the token handle is owned by this RAII wrapper and closed exactly once.
            let _ = unsafe { CloseHandle(self.handle) };
        }
    }
}

#[derive(Debug)]
struct LocalString {
    value: PWSTR,
}

impl LocalString {
    fn new(value: PWSTR) -> Result<Self, WindowsUserError> {
        if value.is_null() {
            Err(WindowsUserError::SidConversionFailed)
        } else {
            Ok(Self { value })
        }
    }

    fn to_string(&self) -> Result<String, WindowsUserError> {
        // SAFETY: ConvertSidToStringSidW returns a NUL-terminated string valid until LocalFree.
        unsafe { self.value.to_string() }.map_err(|_| WindowsUserError::InvalidSidString)
    }
}

impl Drop for LocalString {
    fn drop(&mut self) {
        if self.value.is_null() {
            return;
        }
        // SAFETY: the string was allocated by ConvertSidToStringSidW and is freed exactly once.
        let _ = unsafe { LocalFree(Some(HLOCAL(self.value.as_ptr().cast()))) };
        self.value = PWSTR::null();
    }
}
