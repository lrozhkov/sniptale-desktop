use std::{
    ffi::OsString,
    io::{self, Read, Write},
    process::ExitCode,
};

#[cfg(any(windows, test))]
use super::resident::run_resident_agent_with_log;
use super::support::{
    write_collect_diagnostics_json, write_doctor_json, write_repair_registration_json,
    write_version_json,
};

#[cfg(all(windows, not(test)))]
mod logging;
mod native_host;
#[cfg(all(windows, not(test)))]
mod native_host_log;
mod parse;

#[cfg(all(windows, not(test)))]
use self::logging::{
    write_agent_run_error_log, write_agent_startup_recovery_logs, write_logger_startup_warning,
};
use self::native_host::run_native_host_mode;
#[cfg(all(windows, not(test)))]
use self::parse::SupportModeOptions;
use self::parse::parse_mode;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ProcessMode {
    Agent,
    NativeHost,
    RepairRegistration,
    VersionJson,
    DoctorJson,
    CollectDiagnostics,
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum ModeError {
    UnsupportedArgument,
    AgentUnavailable,
    NativeHostUnavailable,
    Io,
    Json,
}

pub(crate) fn run(args: impl IntoIterator<Item = OsString>) -> ExitCode {
    let mut stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    match run_with_streams(args, &mut stdin, &mut stdout, &mut stderr) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => error.exit_code(),
    }
}

#[cfg(test)]
pub(super) fn run_with_io(
    args: impl IntoIterator<Item = OsString>,
    stdout: &mut (impl Write + Send),
    stderr: &mut impl Write,
) -> Result<(), ModeError> {
    run_with_streams(args, &mut io::empty(), stdout, stderr)
}

pub(super) fn run_with_streams(
    args: impl IntoIterator<Item = OsString>,
    stdin: &mut (impl Read + Send),
    stdout: &mut (impl Write + Send),
    stderr: &mut impl Write,
) -> Result<(), ModeError> {
    let parsed = match parse_mode(args) {
        Ok(parsed) => parsed,
        Err(error) => {
            write_error(stderr, &error)?;
            return Err(error);
        }
    };
    let mode = parsed.mode;
    #[cfg(not(test))]
    crate::diagnostics::install_panic_logger_for_mode(mode.as_str());
    #[cfg(all(windows, not(test)))]
    let mut process_logger = start_process_logger(mode, &parsed.support_options, stderr)?;
    #[cfg(all(windows, not(test)))]
    if mode == ProcessMode::Agent
        && let Some(logger) = process_logger.as_mut()
    {
        write_agent_startup_recovery_logs(logger);
    }
    match mode {
        ProcessMode::Agent => {
            set_mode_crash_context(mode, "startup", None, "agent-run");
            #[cfg(not(any(windows, test)))]
            {
                if write_error(stderr, &ModeError::AgentUnavailable).is_err() {
                    Err(ModeError::Io)
                } else {
                    Err(ModeError::AgentUnavailable)
                }
            }
            #[cfg(any(windows, test))]
            {
                #[cfg(not(test))]
                let agent_log = process_logger
                    .as_ref()
                    .map(crate::diagnostics::ProcessLogger::sink);
                #[cfg(test)]
                let agent_log = None;
                match run_resident_agent_with_log(agent_log.as_ref()) {
                    Ok(_) => Ok(()),
                    Err(error) => {
                        #[cfg(not(test))]
                        if let Some(logger) = process_logger.as_mut() {
                            write_agent_run_error_log(logger, &error);
                        }
                        #[cfg(test)]
                        let _ = &error;
                        if write_error(stderr, &ModeError::AgentUnavailable).is_err() {
                            Err(ModeError::Io)
                        } else {
                            Err(ModeError::AgentUnavailable)
                        }
                    }
                }
            }
        }
        ProcessMode::VersionJson => {
            set_mode_crash_context(mode, "startup", None, "version-json");
            write_version_json(stdout)
        }
        ProcessMode::RepairRegistration => {
            set_mode_crash_context(mode, "install-health", None, "repair-registration");
            write_repair_registration_json(stdout)
        }
        ProcessMode::DoctorJson => {
            set_mode_crash_context(mode, "install-health", None, "doctor-json");
            write_doctor_json(stdout)
        }
        ProcessMode::CollectDiagnostics => {
            set_mode_crash_context(mode, "install-health", None, "collect-diagnostics");
            write_collect_diagnostics_json(stdout)
        }
        ProcessMode::NativeHost => {
            set_mode_crash_context(mode, "handshake", None, "native-host-bridge");
            #[cfg(all(windows, not(test)))]
            {
                run_native_host_mode(
                    parsed.native_host_origin.as_deref(),
                    stdin,
                    stdout,
                    stderr,
                    process_logger.as_mut(),
                )
            }
            #[cfg(any(not(windows), test))]
            {
                run_native_host_mode(parsed.native_host_origin.as_deref(), stdin, stdout, stderr)
            }
        }
    }
}

#[cfg(not(test))]
fn set_mode_crash_context(
    mode: ProcessMode,
    operation: &'static str,
    operation_id: Option<String>,
    phase: &'static str,
) {
    crate::diagnostics::set_crash_context(crate::diagnostics::CrashContext::with_phase(
        mode.as_str(),
        operation,
        operation_id,
        phase,
    ));
}

#[cfg(test)]
fn set_mode_crash_context(
    _mode: ProcessMode,
    _operation: &'static str,
    _operation_id: Option<String>,
    _phase: &'static str,
) {
}

#[cfg(all(windows, not(test)))]
fn start_process_logger(
    mode: ProcessMode,
    options: &SupportModeOptions,
    stderr: &mut impl Write,
) -> Result<Option<crate::diagnostics::ProcessLogger>, ModeError> {
    match crate::diagnostics::start_process_logger_for_mode_with_options(
        mode.as_str(),
        options.log_level.clone(),
        options.log_dir.clone(),
    ) {
        Ok(logger) => Ok(Some(logger)),
        Err(error) => {
            write_logger_startup_warning(stderr, mode, error)?;
            Ok(None)
        }
    }
}

pub(super) fn write_error(stderr: &mut impl Write, error: &ModeError) -> Result<(), ModeError> {
    let message = match error {
        ModeError::UnsupportedArgument => "unsupported argument\n",
        ModeError::AgentUnavailable => "agent unavailable\n",
        ModeError::NativeHostUnavailable => "native host bridge unavailable\n",
        ModeError::Io | ModeError::Json => "output error\n",
    };
    stderr
        .write_all(message.as_bytes())
        .map_err(|_| ModeError::Io)
}

impl ModeError {
    fn exit_code(&self) -> ExitCode {
        match self {
            Self::UnsupportedArgument => ExitCode::from(64),
            Self::AgentUnavailable => ExitCode::from(70),
            Self::NativeHostUnavailable => ExitCode::from(69),
            Self::Io | Self::Json => ExitCode::from(74),
        }
    }
}

impl ProcessMode {
    pub(super) const fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::NativeHost => "native-host",
            Self::RepairRegistration => "repair-registration",
            Self::VersionJson => "version-json",
            Self::DoctorJson => "doctor-json",
            Self::CollectDiagnostics => "collect-diagnostics",
        }
    }
}
