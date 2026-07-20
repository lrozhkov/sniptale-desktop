//! Windows entropy adapter for authenticated pipe challenges.
#![cfg(windows)]
#![allow(unsafe_code)]

use windows::Win32::Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowsPipeEntropyError {
    GenerateFailed,
}

pub(crate) fn random_pipe_auth_bytes<const N: usize>() -> Result<[u8; N], WindowsPipeEntropyError> {
    let mut bytes = [0_u8; N];
    // SAFETY: `bytes` is a valid mutable byte slice for the duration of the call; `None` with
    // `BCRYPT_USE_SYSTEM_PREFERRED_RNG` delegates nonce generation to the Windows system RNG.
    let status = unsafe { BCryptGenRandom(None, &mut bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    if status.0 >= 0 {
        Ok(bytes)
    } else {
        Err(WindowsPipeEntropyError::GenerateFailed)
    }
}
