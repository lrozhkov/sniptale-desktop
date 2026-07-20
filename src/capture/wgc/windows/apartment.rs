//! WinRT apartment guard for Windows Graphics Capture workers.
#![allow(unsafe_code)]

use std::cell::OnceCell;

use windows::Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize};

use super::super::super::screenshot::ScreenshotCaptureError;

thread_local! {
    // WinRT factories used by windows-rs may outlive one capture call. Tearing the apartment down
    // after a screenshot leaves those thread-bound COM interfaces unusable on the next capture.
    // The owner therefore follows the worker thread, while per-session tokens only prove that the
    // current thread has initialized WinRT.
    static WINRT_APARTMENT: OnceCell<WinrtApartmentOwner> = const { OnceCell::new() };
}

#[derive(Debug)]
struct WinrtApartmentOwner;

#[derive(Debug)]
pub(super) struct WinrtApartment;

impl WinrtApartment {
    pub(super) fn initialize() -> Result<Self, ScreenshotCaptureError> {
        WINRT_APARTMENT
            .try_with(|owner| {
                if owner.get().is_none() {
                    // SAFETY: initializes WinRT for this thread. The thread-local owner below
                    // calls RoUninitialize once after every WGC session on the thread is gone.
                    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
                        .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
                    if owner.set(WinrtApartmentOwner).is_err() {
                        // SAFETY: balances the successful initialization above if TLS ownership
                        // could not be established; the existing owner remains authoritative.
                        unsafe { RoUninitialize() };
                    }
                }
                Ok(Self)
            })
            .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?
    }
}

impl Drop for WinrtApartmentOwner {
    fn drop(&mut self) {
        // SAFETY: this thread-local owner is installed only after RoInitialize succeeds on the
        // same thread, and TLS destruction runs on that thread after its capture owners drop.
        unsafe { RoUninitialize() };
    }
}
