//! Process log writer lifecycle.

use std::{
    error::Error,
    fmt,
    fs::File,
    io::Write,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

#[cfg(test)]
use crate::diagnostics::log::file::LogFileLifecycleOutcome;
use crate::{
    diagnostics::log::{
        file::{LogFileLifecycleError, PreparedLogFile, prepare_managed_log_file},
        record::{
            LogErrorFields, LogLevel, StructuredLogContext, build_structured_log_record,
            serialize_log_record_json_line,
        },
        setup::{LogSetupError, LogSetupInputs, LogSetupPlan, plan_process_logging},
    },
    protocol::{CACHE_SCHEMA_VERSION, PROTOCOL_VERSION, SETTINGS_SCHEMA_VERSION},
};

#[derive(Debug)]
pub(crate) struct ProcessLogger {
    sink: ProcessLogSink,
    #[cfg(test)]
    outcome: LogFileLifecycleOutcome,
}

#[derive(Clone, Debug)]
pub(crate) struct ProcessLogSink {
    file: Arc<Mutex<File>>,
    plan: LogSetupPlan,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProcessLoggerError {
    Setup(LogSetupError),
    File(LogFileLifecycleError),
    Clock,
    Json,
    Io,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ProcessLogEvent<'a> {
    pub(crate) level: LogLevel,
    pub(crate) target: &'a str,
    pub(crate) operation: &'static str,
    pub(crate) operation_id: Option<&'a str>,
    pub(crate) phase: &'static str,
    pub(crate) message: &'a str,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) error: Option<&'a LogErrorFields>,
}

pub(crate) fn start_process_logger_for_mode_with_options(
    process_mode: &'static str,
    cli_level: Option<String>,
    cli_log_dir: Option<PathBuf>,
) -> Result<ProcessLogger, ProcessLoggerError> {
    let inputs = LogSetupInputs {
        process_mode,
        local_app_data_dir: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
        temp_dir: std::env::temp_dir(),
        env_level: std::env::var("SNIPTALE_NATIVE_LOG").ok(),
        cli_level,
        cli_log_dir,
        debug_build: cfg!(debug_assertions),
    };
    start_process_logger(&inputs, system_epoch_ms()?, std::process::id())
}

pub(crate) fn start_process_logger(
    inputs: &LogSetupInputs,
    now_epoch_ms: u64,
    process_id: u32,
) -> Result<ProcessLogger, ProcessLoggerError> {
    let plan = plan_process_logging(inputs).map_err(ProcessLoggerError::Setup)?;
    let PreparedLogFile { file, outcome } =
        prepare_managed_log_file(&plan.directory, now_epoch_ms, process_id)
            .map_err(ProcessLoggerError::File)?;
    #[cfg(not(test))]
    let _ = outcome;
    let mut logger = ProcessLogger {
        sink: ProcessLogSink {
            file: Arc::new(Mutex::new(file)),
            plan,
        },
        #[cfg(test)]
        outcome,
    };
    logger.write_event(now_epoch_ms, ProcessLogEvent::startup())?;
    Ok(logger)
}

impl ProcessLogger {
    #[cfg(any(all(windows, not(test)), test))]
    pub(crate) fn sink(&self) -> ProcessLogSink {
        self.sink.clone()
    }

    pub(crate) fn write_event(
        &mut self,
        timestamp_epoch_ms: u64,
        event: ProcessLogEvent<'_>,
    ) -> Result<(), ProcessLoggerError> {
        self.sink.write_event(timestamp_epoch_ms, event)
    }

    pub(crate) fn write_event_now(
        &mut self,
        event: ProcessLogEvent<'_>,
    ) -> Result<(), ProcessLoggerError> {
        self.write_event(system_epoch_ms()?, event)
    }

    #[cfg(any(all(windows, not(test)), test))]
    pub(crate) fn write_startup_recording_recovery_summary(
        &mut self,
        timestamp_epoch_ms: u64,
        pending_recordings: usize,
        keep_writing: usize,
        retained_rejected: usize,
        deleted: usize,
    ) -> Result<(), ProcessLoggerError> {
        let message = format!(
            "temp-cache recovery scan pendingRecordings={pending_recordings} keepWriting={keep_writing} retainedRejected={retained_rejected} deleted={deleted}",
        );
        self.write_event(
            timestamp_epoch_ms,
            ProcessLogEvent {
                level: LogLevel::Info,
                target: "sniptale_native_agent::startup_recovery",
                operation: "startup",
                operation_id: None,
                phase: "recovery-scan",
                message: &message,
                duration_ms: None,
                error: None,
            },
        )
    }

    #[cfg(any(all(windows, not(test)), test))]
    pub(crate) fn write_startup_recording_recovery_summary_now(
        &mut self,
        pending_recordings: usize,
        keep_writing: usize,
        retained_rejected: usize,
        deleted: usize,
    ) -> Result<(), ProcessLoggerError> {
        self.write_startup_recording_recovery_summary(
            system_epoch_ms()?,
            pending_recordings,
            keep_writing,
            retained_rejected,
            deleted,
        )
    }

    #[cfg(any(all(windows, not(test)), test))]
    pub(crate) fn write_startup_screenshot_recovery_summary(
        &mut self,
        timestamp_epoch_ms: u64,
        pending_screenshots: usize,
        keep_writing: usize,
        retained_rejected: usize,
        deleted: usize,
    ) -> Result<(), ProcessLoggerError> {
        let message = format!(
            "screenshot temp-cache recovery scan pendingScreenshots={pending_screenshots} keepWriting={keep_writing} retainedRejected={retained_rejected} deleted={deleted}",
        );
        self.write_event(
            timestamp_epoch_ms,
            ProcessLogEvent {
                level: LogLevel::Info,
                target: "sniptale_native_agent::startup_recovery",
                operation: "startup",
                operation_id: None,
                phase: "screenshot-recovery-scan",
                message: &message,
                duration_ms: None,
                error: None,
            },
        )
    }

    #[cfg(any(all(windows, not(test)), test))]
    pub(crate) fn write_startup_screenshot_recovery_summary_now(
        &mut self,
        pending_screenshots: usize,
        keep_writing: usize,
        retained_rejected: usize,
        deleted: usize,
    ) -> Result<(), ProcessLoggerError> {
        self.write_startup_screenshot_recovery_summary(
            system_epoch_ms()?,
            pending_screenshots,
            keep_writing,
            retained_rejected,
            deleted,
        )
    }

    #[cfg(test)]
    pub(crate) const fn outcome(&self) -> &LogFileLifecycleOutcome {
        &self.outcome
    }
}

impl ProcessLogSink {
    #[cfg(any(all(windows, not(test)), test))]
    pub(crate) fn write_event_now(
        &self,
        event: ProcessLogEvent<'_>,
    ) -> Result<(), ProcessLoggerError> {
        self.write_event(system_epoch_ms()?, event)
    }

    pub(crate) fn write_event(
        &self,
        timestamp_epoch_ms: u64,
        event: ProcessLogEvent<'_>,
    ) -> Result<(), ProcessLoggerError> {
        let context = StructuredLogContext {
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            protocol_version: PROTOCOL_VERSION,
            settings_schema_version: SETTINGS_SCHEMA_VERSION,
            app_cache_schema_version: CACHE_SCHEMA_VERSION,
            platform_kind: std::env::consts::OS.to_owned(),
            process_mode: self.plan.process_mode.to_owned(),
            process_id: std::process::id(),
            connection_id: None,
            controller_lease_id: None,
            operation: event.operation,
            operation_id: event.operation_id.map(str::to_owned),
            phase: event.phase,
        };
        let record = build_structured_log_record(
            timestamp_epoch_ms,
            event.level,
            event.target,
            context,
            event.message,
            event.duration_ms,
            event.error.cloned(),
        );
        let line = serialize_log_record_json_line(&record).map_err(|_| ProcessLoggerError::Json)?;
        self.file
            .lock()
            .map_err(|_| ProcessLoggerError::Io)?
            .write_all(line.as_bytes())
            .map_err(|_| ProcessLoggerError::Io)
    }
}

impl ProcessLogEvent<'_> {
    const fn startup() -> Self {
        Self {
            level: LogLevel::Info,
            target: "sniptale_native_agent::startup",
            operation: "startup",
            operation_id: None,
            phase: "logging",
            message: "process logging initialized",
            duration_ms: None,
            error: None,
        }
    }
}

impl fmt::Display for ProcessLoggerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Setup(error) => write!(formatter, "process log setup failed: {error:?}"),
            Self::File(error) => write!(formatter, "process log file failed: {error}"),
            Self::Clock => formatter.write_str("process log clock failed"),
            Self::Json => formatter.write_str("process log JSON serialization failed"),
            Self::Io => formatter.write_str("process log write failed"),
        }
    }
}

impl Error for ProcessLoggerError {}

fn system_epoch_ms() -> Result<u64, ProcessLoggerError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ProcessLoggerError::Clock)?
        .as_millis()
        .try_into()
        .map_err(|_| ProcessLoggerError::Clock)
}
