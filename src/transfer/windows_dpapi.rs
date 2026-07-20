//! Windows DPAPI current-user wrapping for per-media encryption keys.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::{ptr, slice};

use windows::{
    Win32::{
        Foundation::{HLOCAL, LocalFree},
        Security::Cryptography::{
            CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
        },
    },
    core::w,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MediaDpapiError {
    EmptyInput,
    InputTooLarge,
    ProtectFailed,
    UnprotectFailed,
    InvalidOutput,
}

pub(crate) fn protect_current_user_media_key(key: &[u8]) -> Result<Vec<u8>, MediaDpapiError> {
    let input = input_blob(key)?;
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: `input` points at the media key for this call only; optional entropy, prompt, and
    // reserved pointers are null; `output` is immediately wrapped for LocalFree ownership.
    unsafe {
        CryptProtectData(
            &input,
            w!("Sniptale temp media key"),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    }
    .map_err(|_| MediaDpapiError::ProtectFailed)?;
    // SAFETY: DPAPI initialized `output` on success; this wrapper owns the LocalAlloc buffer.
    let protected = unsafe { LocalBlob::new(output, false)? };
    protected.to_vec()
}

pub(crate) fn unprotect_current_user_media_key(
    protected: &[u8],
) -> Result<Vec<u8>, MediaDpapiError> {
    let input = input_blob(protected)?;
    let mut output = CRYPT_INTEGER_BLOB::default();
    // SAFETY: `input` points at the DPAPI payload for this call only; no UI is allowed; `output`
    // is immediately wrapped and wiped before LocalFree on drop.
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
    .map_err(|_| MediaDpapiError::UnprotectFailed)?;
    // SAFETY: DPAPI initialized `output` on success; this wrapper owns and wipes the key buffer.
    let key = unsafe { LocalBlob::new(output, true)? };
    key.to_vec()
}

fn input_blob(input: &[u8]) -> Result<CRYPT_INTEGER_BLOB, MediaDpapiError> {
    if input.is_empty() {
        return Err(MediaDpapiError::EmptyInput);
    }
    let cb_data = u32::try_from(input.len()).map_err(|_| MediaDpapiError::InputTooLarge)?;
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
    unsafe fn new(blob: CRYPT_INTEGER_BLOB, wipe_on_drop: bool) -> Result<Self, MediaDpapiError> {
        if blob.pbData.is_null() || blob.cbData == 0 {
            Err(MediaDpapiError::InvalidOutput)
        } else {
            Ok(Self { blob, wipe_on_drop })
        }
    }

    fn to_vec(&self) -> Result<Vec<u8>, MediaDpapiError> {
        let length =
            usize::try_from(self.blob.cbData).map_err(|_| MediaDpapiError::InvalidOutput)?;
        if self.blob.pbData.is_null() || length == 0 {
            return Err(MediaDpapiError::InvalidOutput);
        }
        // SAFETY: `LocalBlob::new` validated the pointer and length; DPAPI owns the allocation for
        // this wrapper's lifetime, and this function only copies bytes out.
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
        assert_eq!(input_blob(&[]), Err(MediaDpapiError::EmptyInput));
    }

    #[test]
    fn local_blob_rejects_invalid_dpapi_output() {
        let invalid = CRYPT_INTEGER_BLOB::default();

        // SAFETY: This passes a null DPAPI output fixture to prove the wrapper rejects it.
        assert!(matches!(
            unsafe { LocalBlob::new(invalid, true) },
            Err(MediaDpapiError::InvalidOutput)
        ));
    }
}
