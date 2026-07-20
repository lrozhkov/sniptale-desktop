//! Runtime logging setup planning.

use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LogSetupInputs {
    pub(crate) process_mode: &'static str,
    pub(crate) local_app_data_dir: Option<PathBuf>,
    pub(crate) temp_dir: PathBuf,
    pub(crate) env_level: Option<String>,
    pub(crate) cli_level: Option<String>,
    pub(crate) cli_log_dir: Option<PathBuf>,
    pub(crate) debug_build: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LogSetupPlan {
    pub(crate) process_mode: &'static str,
    pub(crate) directory: PathBuf,
    pub(crate) directory_category: &'static str,
    pub(crate) level: LogLevelSetting,
    pub(crate) json_file_enabled: bool,
    pub(crate) console_enabled: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LogLevelSetting {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LogSetupError {
    InvalidEnvLevel,
}

pub(crate) fn plan_process_logging(inputs: &LogSetupInputs) -> Result<LogSetupPlan, LogSetupError> {
    let (directory, directory_category) = log_directory(inputs);
    Ok(LogSetupPlan {
        process_mode: inputs.process_mode,
        directory,
        directory_category,
        level: log_level(
            inputs.process_mode,
            inputs.cli_level.as_deref(),
            inputs.env_level.as_deref(),
            inputs.debug_build,
        )?,
        json_file_enabled: true,
        console_enabled: console_enabled(inputs.process_mode),
    })
}

fn log_directory(inputs: &LogSetupInputs) -> (PathBuf, &'static str) {
    if support_mode(inputs.process_mode)
        && let Some(directory) = &inputs.cli_log_dir
    {
        return (directory.clone(), "custom-log-dir");
    }
    let directory_root = inputs
        .local_app_data_dir
        .clone()
        .unwrap_or_else(|| inputs.temp_dir.clone());
    (
        directory_root
            .join("Sniptale")
            .join("NativeAgent")
            .join("logs"),
        "local-app-data-logs",
    )
}

fn log_level(
    process_mode: &str,
    cli_level: Option<&str>,
    env_level: Option<&str>,
    debug_build: bool,
) -> Result<LogLevelSetting, LogSetupError> {
    if support_mode(process_mode)
        && let Some(level) = cli_level.or(env_level)
    {
        return parse_level(level);
    }
    if debug_build {
        Ok(LogLevelSetting::Debug)
    } else {
        Ok(LogLevelSetting::Info)
    }
}

fn parse_level(level: &str) -> Result<LogLevelSetting, LogSetupError> {
    match level.to_ascii_lowercase().as_str() {
        "trace" => Ok(LogLevelSetting::Trace),
        "debug" => Ok(LogLevelSetting::Debug),
        "info" => Ok(LogLevelSetting::Info),
        "warn" => Ok(LogLevelSetting::Warn),
        "error" => Ok(LogLevelSetting::Error),
        _ => Err(LogSetupError::InvalidEnvLevel),
    }
}

fn support_mode(process_mode: &str) -> bool {
    matches!(
        process_mode,
        "repair-registration" | "version-json" | "doctor-json" | "collect-diagnostics"
    )
}

fn console_enabled(process_mode: &str) -> bool {
    matches!(
        process_mode,
        "repair-registration" | "doctor-json" | "collect-diagnostics"
    )
}
