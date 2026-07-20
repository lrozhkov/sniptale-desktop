use std::path::PathBuf;

use super::super::log::setup::{
    LogLevelSetting, LogSetupError, LogSetupInputs, plan_process_logging,
};

#[test]
fn native_host_logging_uses_file_output_without_console() -> Result<(), LogSetupError> {
    let plan = plan_process_logging(&inputs("native-host", false, Some("trace")))?;

    assert!(plan.json_file_enabled);
    assert!(!plan.console_enabled);
    assert_eq!(plan.level, LogLevelSetting::Info);
    assert_eq!(plan.directory_category, "local-app-data-logs");
    assert_eq!(
        plan.directory,
        PathBuf::from("local-app-data")
            .join("Sniptale")
            .join("NativeAgent")
            .join("logs")
    );
    Ok(())
}

#[test]
fn support_logging_accepts_env_level_override() -> Result<(), LogSetupError> {
    let plan = plan_process_logging(&inputs("collect-diagnostics", false, Some("trace")))?;

    assert_eq!(plan.level, LogLevelSetting::Trace);
    assert!(plan.console_enabled);
    Ok(())
}

#[test]
fn support_logging_cli_options_override_level_and_directory() -> Result<(), LogSetupError> {
    let mut inputs = inputs("doctor-json", false, Some("error"));
    inputs.cli_level = Some("debug".to_owned());
    inputs.cli_log_dir = Some(PathBuf::from("support-logs"));

    let plan = plan_process_logging(&inputs)?;

    assert_eq!(plan.level, LogLevelSetting::Debug);
    assert_eq!(plan.directory, PathBuf::from("support-logs"));
    assert_eq!(plan.directory_category, "custom-log-dir");
    Ok(())
}

#[test]
fn non_support_logging_ignores_cli_overrides() -> Result<(), LogSetupError> {
    let mut inputs = inputs("native-host", false, Some("trace"));
    inputs.cli_level = Some("debug".to_owned());
    inputs.cli_log_dir = Some(PathBuf::from("support-logs"));

    let plan = plan_process_logging(&inputs)?;

    assert_eq!(plan.level, LogLevelSetting::Info);
    assert_eq!(plan.directory_category, "local-app-data-logs");
    Ok(())
}

#[test]
fn logging_defaults_to_debug_for_dev_and_info_for_release() -> Result<(), LogSetupError> {
    let dev = plan_process_logging(&inputs("agent", true, None))?;
    let release = plan_process_logging(&inputs("agent", false, None))?;

    assert_eq!(dev.level, LogLevelSetting::Debug);
    assert_eq!(release.level, LogLevelSetting::Info);
    assert!(!dev.console_enabled);
    assert!(!release.console_enabled);
    Ok(())
}

#[test]
fn logging_rejects_invalid_support_env_level() {
    assert_eq!(
        plan_process_logging(&inputs("doctor-json", false, Some("verbose"))),
        Err(LogSetupError::InvalidEnvLevel)
    );
}

#[test]
fn logging_falls_back_to_temp_root_when_local_app_data_is_missing() -> Result<(), LogSetupError> {
    let mut inputs = inputs("agent", false, None);
    inputs.local_app_data_dir = None;

    let plan = plan_process_logging(&inputs)?;

    assert_eq!(
        plan.directory,
        PathBuf::from("temp")
            .join("Sniptale")
            .join("NativeAgent")
            .join("logs")
    );
    Ok(())
}

fn inputs(
    process_mode: &'static str,
    debug_build: bool,
    env_level: Option<&str>,
) -> LogSetupInputs {
    LogSetupInputs {
        process_mode,
        local_app_data_dir: Some(PathBuf::from("local-app-data")),
        temp_dir: PathBuf::from("temp"),
        env_level: env_level.map(str::to_owned),
        cli_level: None,
        cli_log_dir: None,
        debug_build,
    }
}
