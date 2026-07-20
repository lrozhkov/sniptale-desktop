//! Native-host process mode runner.

use std::io::{Read, Write};

use super::{ModeError, write_error};
use crate::app::bridge::{NativeHostRuntimeLog, run_native_host_bridge};

#[cfg(all(windows, not(test)))]
use super::logging::write_native_host_run_error_log;
#[cfg(all(windows, not(test)))]
use super::native_host_log::NativeHostProcessLog;
#[cfg(all(windows, not(test)))]
use crate::app::bridge::NativeHostRunError;

#[cfg(all(windows, not(test)))]
pub(super) fn run_native_host_mode(
    origin: Option<&str>,
    stdin: &mut (impl Read + Send),
    stdout: &mut (impl Write + Send),
    stderr: &mut impl Write,
    process_logger: Option<&mut crate::diagnostics::ProcessLogger>,
) -> Result<(), ModeError> {
    if !extension_admission_allowed() {
        let _ = write_error(stderr, &ModeError::NativeHostUnavailable);
        return Err(ModeError::NativeHostUnavailable);
    }
    let Some(origin) = origin else {
        let error = NativeHostRunError::PipeAuthenticationUnavailable;
        return Err(native_host_bridge_mode_error(
            stderr,
            process_logger,
            &error,
        ));
    };
    let native_host_process_log = process_logger
        .as_ref()
        .map(|logger| logger.sink())
        .map(NativeHostProcessLog);
    let native_host_log = native_host_process_log
        .as_ref()
        .map(|log| log as &dyn NativeHostRuntimeLog);
    run_native_host_bridge(stdin, stdout, origin, native_host_log)
        .map_err(|error| native_host_bridge_mode_error(stderr, process_logger, &error))
}

#[cfg(any(not(windows), test))]
pub(super) fn run_native_host_mode(
    origin: Option<&str>,
    stdin: &mut (impl Read + Send),
    stdout: &mut (impl Write + Send),
    stderr: &mut impl Write,
) -> Result<(), ModeError> {
    if !extension_admission_allowed() {
        let _ = write_error(stderr, &ModeError::NativeHostUnavailable);
        return Err(ModeError::NativeHostUnavailable);
    }
    let Some(origin) = origin else {
        let _ = write_error(stderr, &ModeError::NativeHostUnavailable);
        return Err(ModeError::NativeHostUnavailable);
    };
    let native_host_log: Option<&dyn NativeHostRuntimeLog> = None;
    run_native_host_bridge(stdin, stdout, origin, native_host_log)
        .map_err(|_| native_host_bridge_mode_error(stderr))
}

#[cfg(all(windows, not(test)))]
fn native_host_bridge_mode_error(
    stderr: &mut impl Write,
    logger: Option<&mut crate::diagnostics::ProcessLogger>,
    error: &NativeHostRunError,
) -> ModeError {
    if let Some(logger) = logger {
        write_native_host_run_error_log(logger, error);
    }
    let _ = write_error(stderr, &ModeError::NativeHostUnavailable);
    ModeError::NativeHostUnavailable
}

#[cfg(any(not(windows), test))]
fn native_host_bridge_mode_error(stderr: &mut impl Write) -> ModeError {
    let _ = write_error(stderr, &ModeError::NativeHostUnavailable);
    ModeError::NativeHostUnavailable
}

fn extension_admission_allowed() -> bool {
    crate::settings::extension_admission_allowed_from_path(
        crate::app::platform::local_profile_path(),
    )
}
