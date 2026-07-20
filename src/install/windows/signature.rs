//! Windows Authenticode signature verification adapter.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::{ffi::OsStr, mem, os::windows::ffi::OsStrExt, ptr};

use windows::{
    Win32::{
        Foundation::{CERT_E_CHAINING, HWND, TRUST_E_NOSIGNATURE},
        Security::WinTrust::{
            WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
            WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_FILE, WTD_DISABLE_MD2_MD4, WTD_REVOKE_NONE,
            WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTD_UICONTEXT_EXECUTE,
            WinVerifyTrustEx,
        },
    },
    core::PCWSTR,
};

use super::super::signature::SignatureProbeResult;

pub(crate) fn verify_file_signature(path: &str) -> SignatureProbeResult {
    if path.is_empty() {
        return SignatureProbeResult::Unknown;
    }
    let path = wide_null(path);
    verify_wide_path(&path)
}

fn verify_wide_path(path: &[u16]) -> SignatureProbeResult {
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: PCWSTR(path.as_ptr()),
        hFile: Default::default(),
        pgKnownSubject: ptr::null_mut(),
    };
    let mut trust_data = WINTRUST_DATA {
        cbStruct: mem::size_of::<WINTRUST_DATA>() as u32,
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 {
            pFile: &mut file_info,
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL | WTD_DISABLE_MD2_MD4,
        dwUIContext: WTD_UICONTEXT_EXECUTE,
        ..Default::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: `trust_data` points to stack-owned WINTRUST_DATA and WINTRUST_FILE_INFO records that
    // remain alive for the call; UI is disabled and the provider state is closed below.
    let status = unsafe { WinVerifyTrustEx(HWND::default(), &mut action, &mut trust_data) };
    trust_data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: this closes provider state opened by the verify call; all pointers still reference
    // live stack data for the duration of the close call.
    let _ = unsafe { WinVerifyTrustEx(HWND::default(), &mut action, &mut trust_data) };
    signature_probe_from_wintrust_status(status)
}

fn signature_probe_from_wintrust_status(status: i32) -> SignatureProbeResult {
    if status == 0 {
        return SignatureProbeResult::Valid;
    }
    if status == TRUST_E_NOSIGNATURE.0 {
        return SignatureProbeResult::Missing;
    }
    if status == CERT_E_CHAINING.0 {
        return SignatureProbeResult::Unknown;
    }
    SignatureProbeResult::Invalid
}

fn wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain([0]).collect()
}
