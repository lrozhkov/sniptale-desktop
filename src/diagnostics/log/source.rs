//! Shared discovery and reads for managed diagnostic logs.

use std::{fs, path::Path};

use super::setup::{LogSetupInputs, plan_process_logging};

const LOG_FILE_PREFIX: &str = "sniptale-native-";
const LOG_FILE_EXTENSION: &str = "jsonl";

pub(in crate::diagnostics) fn default_log_directory() -> std::path::PathBuf {
    let inputs = LogSetupInputs {
        process_mode: "agent",
        local_app_data_dir: std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from),
        temp_dir: std::env::temp_dir(),
        env_level: None,
        cli_level: None,
        cli_log_dir: None,
        debug_build: cfg!(debug_assertions),
    };
    plan_process_logging(&inputs).map_or_else(
        |_| {
            std::env::temp_dir()
                .join("Sniptale")
                .join("NativeAgent")
                .join("logs")
        },
        |plan| plan.directory,
    )
}

pub(in crate::diagnostics) fn read_log_file_best_effort(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

pub(in crate::diagnostics) fn managed_log_files(directory: &Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_name = entry.file_name().to_string_lossy().into_owned();
            if managed_log_file_name(&file_name) {
                Some(entry.path())
            } else {
                None
            }
        })
        .collect()
}

fn managed_log_file_name(file_name: &str) -> bool {
    file_name.starts_with(LOG_FILE_PREFIX)
        && Path::new(file_name)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|extension| extension.eq_ignore_ascii_case(LOG_FILE_EXTENSION))
        && !file_name.contains(['/', '\\', ':'])
}
