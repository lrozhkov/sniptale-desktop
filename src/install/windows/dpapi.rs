//! Windows DPAPI current-user secret protection adapter.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::{ptr, slice};

use windows::Win32::{
    Foundation::{HLOCAL, LocalFree},
    Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DpapiSecretError {
    EmptyInput,
    InputTooLarge,
    UnprotectFailed,
    ProtectFailed,
    InvalidOutput,
}

pub(crate) fn protect_current_user_pipe_secret(secret: &[u8]) -> Result<Vec<u8>, DpapiSecretError> {
    let input = input_blob(secret)?;
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: input borrows the live secret; output is initialized by DPAPI, remains adapter-local,
    // and is owned by LocalBlob until its matching LocalFree. No UI or machine scope is requested.
    unsafe {
        CryptProtectData(
            &input,
            windows::core::w!("Sniptale pipe secret"),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    }
    .map_err(|_| DpapiSecretError::ProtectFailed)?;
    // SAFETY: the successful DPAPI call allocated output; LocalBlob frees it on every return path.
    let protected = unsafe { LocalBlob::new(output, false)? };
    protected.to_vec()
}

pub(crate) fn unprotect_current_user_secret(protected: &[u8]) -> Result<Vec<u8>, DpapiSecretError> {
    let input = input_blob(protected)?;
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: `input` points at `protected` for the duration of the call; no prompt or entropy is
    // supplied; `output` is immediately wrapped and wiped before LocalFree on drop.
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    }
    .map_err(|_| DpapiSecretError::UnprotectFailed)?;
    // SAFETY: DPAPI initialized `output` on success; this wrapper owns and wipes the secret buffer.
    let secret = unsafe { LocalBlob::new(output, true)? };
    secret.to_vec()
}

fn input_blob(input: &[u8]) -> Result<CRYPT_INTEGER_BLOB, DpapiSecretError> {
    if input.is_empty() {
        return Err(DpapiSecretError::EmptyInput);
    }
    let cb_data = u32::try_from(input.len()).map_err(|_| DpapiSecretError::InputTooLarge)?;
    Ok(CRYPT_INTEGER_BLOB {
        cbData: cb_data,
        pbData: input.as_ptr().cast_mut(),
    })
}

#[derive(Debug)]
struct LocalBlob {
    blob: CRYPT_INTEGER_BLOB,
    wipe_on_drop: bool,
}

impl LocalBlob {
    unsafe fn new(blob: CRYPT_INTEGER_BLOB, wipe_on_drop: bool) -> Result<Self, DpapiSecretError> {
        if blob.pbData.is_null() || blob.cbData == 0 {
            Err(DpapiSecretError::InvalidOutput)
        } else {
            Ok(Self { blob, wipe_on_drop })
        }
    }

    fn to_vec(&self) -> Result<Vec<u8>, DpapiSecretError> {
        let length =
            usize::try_from(self.blob.cbData).map_err(|_| DpapiSecretError::InvalidOutput)?;
        if self.blob.pbData.is_null() || length == 0 {
            return Err(DpapiSecretError::InvalidOutput);
        }
        // SAFETY: `LocalBlob::new` validated the pointer and length; DPAPI owns the allocation for
        // at least this wrapper's lifetime, and this function only copies bytes out.
        Ok(unsafe { slice::from_raw_parts(self.blob.pbData, length) }.to_vec())
    }
}

impl Drop for LocalBlob {
    fn drop(&mut self) {
        if self.blob.pbData.is_null() {
            return;
        }
        if self.wipe_on_drop {
            // SAFETY: `pbData` points to a DPAPI LocalAlloc buffer owned by this wrapper; writing
            // zeros across `cbData` bytes occurs before the matching LocalFree.
            unsafe { ptr::write_bytes(self.blob.pbData, 0, self.blob.cbData as usize) };
        }
        // SAFETY: `pbData` was allocated by DPAPI and is freed exactly once by this RAII owner.
        let _ = unsafe { LocalFree(Some(HLOCAL(self.blob.pbData.cast()))) };
        self.blob.pbData = ptr::null_mut();
        self.blob.cbData = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_blob_rejects_empty_input_before_dpapi() {
        assert_eq!(input_blob(&[]), Err(DpapiSecretError::EmptyInput));
    }

    #[test]
    fn local_blob_rejects_invalid_dpapi_output() {
        let invalid = CRYPT_INTEGER_BLOB::default();

        // SAFETY: This passes a null DPAPI output fixture to prove the wrapper rejects it.
        assert!(matches!(
            unsafe { LocalBlob::new(invalid, true) },
            Err(DpapiSecretError::InvalidOutput)
        ));
    }
}
