//! Explicitly-invoked Windows acceptance probes for privileged WGC behavior.

use super::{apartment::WinrtApartment, readback::request_borderless_access};

#[test]
#[ignore = "requires live installed-Windows borderless capture consent"]
fn live_windows_process_receives_borderless_capture_access() -> Result<(), String> {
    let _winrt = WinrtApartment::initialize()
        .map_err(|error| format!("WinRT initialization failed: {error:?}"))?;
    if request_borderless_access() {
        Ok(())
    } else {
        Err("Windows did not grant borderless capture access".to_owned())
    }
}
