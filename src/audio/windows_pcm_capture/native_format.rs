//! Owner-local lifetime for the shared audio engine format allocation.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::ptr::NonNull;

use windows::Win32::{
    Media::Audio::{IAudioClient, WAVEFORMATEX},
    System::Com::CoTaskMemFree,
};

use super::WindowsWasapiPcmError;

struct NativeMixFormat(NonNull<WAVEFORMATEX>);

impl Drop for NativeMixFormat {
    fn drop(&mut self) {
        // SAFETY: GetMixFormat transfers this allocation to the caller; this owner frees it once.
        unsafe { CoTaskMemFree(Some(self.0.as_ptr().cast())) };
    }
}

pub(super) fn device_sample_rate(client: &IAudioClient) -> Result<u32, WindowsWasapiPcmError> {
    // SAFETY: the calling thread owns a valid initialized COM apartment and audio client.
    let pointer = unsafe { client.GetMixFormat() }
        .map_err(|error| WindowsWasapiPcmError::InitializeFailed(error.code().0))?;
    let format =
        NativeMixFormat(NonNull::new(pointer).ok_or(WindowsWasapiPcmError::InvalidFormat)?);
    // SAFETY: GetMixFormat returned a non-null allocation containing at least WAVEFORMATEX.
    let rate = unsafe { format.0.as_ref().nSamplesPerSec };
    if rate == 0 {
        return Err(WindowsWasapiPcmError::InvalidFormat);
    }
    Ok(rate)
}
