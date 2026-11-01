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

/// Static fields prevent untrusted parser details from entering process logs.
#[cfg(any(windows, test))]
pub(super) struct NativeHostFailureSummary {
    pub(super) code: &'static str,
    pub(super) message: &'static str,
}

#[cfg(any(windows, test))]
pub(super) fn native_host_failure_summary(
    error: &crate::app::bridge::NativeHostRunError,
) -> NativeHostFailureSummary {
    use crate::app::bridge::NativeHostRunError;
    let code = match error {
        NativeHostRunError::AuthenticatedBridge(error) => match error {
            crate::ipc::AuthenticatedBridgeForwardError::Handshake(_) => "native_host.pipe_auth",
            crate::ipc::AuthenticatedBridgeForwardError::Split(_) => "native_host.pipe_split",
            crate::ipc::AuthenticatedBridgeForwardError::Forward(_) => "native_host.forward",
        },
        #[cfg(all(windows, not(test)))]
        NativeHostRunError::InstallIdentityDrift(_) => "native_host.install_identity_drift",
        #[cfg(all(windows, not(test)))]
        NativeHostRunError::PipeAuthenticationUnavailable => "native_host.pipe_auth_unavailable",
        #[cfg(any(not(windows), test))]
        NativeHostRunError::Frame(_) => "native_host.frame",
        #[cfg(any(not(windows), test))]
        NativeHostRunError::Protocol(_) => "native_host.protocol",
        #[cfg(any(not(windows), test))]
        NativeHostRunError::Json(_) => "native_host.json",
    };
    NativeHostFailureSummary {
        code,
        message: "native host bridge failed",
    }
}

#[cfg(test)]
mod log_privacy_tests {
    use super::*;
    #[test]
    fn parser_details_cannot_enter_native_host_failure_fields()
    -> Result<(), Box<dyn std::error::Error>> {
        let needle = "private-never-log.invalid";
        let payload = br#"{"type":"https://private-never-log.invalid/token","protocolVersion":2}"#;
        let Err(error) = crate::protocol::parse_wire_message(payload) else {
            return Err("malformed message unexpectedly accepted".into());
        };
        let errors = [
            (
                crate::app::bridge::NativeHostRunError::Protocol(error.clone()),
                "native_host.protocol",
            ),
            (
                crate::app::bridge::NativeHostRunError::AuthenticatedBridge(
                    crate::ipc::AuthenticatedBridgeForwardError::Forward(
                        crate::ipc::BridgeForwardError::BrowserProtocol(error),
                    ),
                ),
                "native_host.forward",
            ),
        ];
        for (error, expected) in errors {
            assert!(format!("{error:?}").contains(needle));
            let summary = native_host_failure_summary(&error);
            assert_eq!(summary.code, expected);
            assert!(!summary.message.contains(needle));
            assert!(!summary.message.contains("token"));
            assert!(summary.message.len() < 128);
        }
        Ok(())
    }
}
