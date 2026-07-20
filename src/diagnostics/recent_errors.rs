//! Redacted recent-error summaries sourced from managed process logs.

use std::{collections::HashMap, fs, path::Path};

use serde::{Deserialize, Serialize};

use super::log::setup::{LogSetupInputs, plan_process_logging};

const MAX_RECENT_ERRORS: usize = 32;
const LOG_FILE_PREFIX: &str = "sniptale-native-";
const LOG_FILE_EXTENSION: &str = "jsonl";
const TIMESTAMP_BUCKET_MS: u64 = 60 * 60 * 1_000;
const ALLOWED_ERROR_CATEGORIES: &[&str] = &[
    "agent.command_channel_closed",
    "agent.event_channel_closed",
    "agent.join_failed",
    "agent.resident_singleton_unavailable",
    "agent.start_failed",
    "agent.start_timeout",
    "logger.clock",
    "logger.file",
    "logger.io",
    "logger.json",
    "logger.setup",
    "native_host.forward",
    "native_host.install_identity_drift",
    "native_host.pipe_auth",
    "native_host.pipe_auth_unavailable",
    "native_host.pipe_split",
    "resident_pipe.handshake_timeout",
    "resident_pipe.session_failed",
    "resident_pipe.session_start_timeout",
    "resident_pipe.session_timeout_no_controller",
    "resident_pipe.session_timeout_no_settings",
    "storage-failed",
    "telemetry.hook_drop",
    "transfer.cleanup_failed",
    "transfer.store_open_failed",
    "unhandled-exception",
    "unhandled-panic",
];
const ALLOWED_PHASES: &[&str] = &[
    "agent-run",
    "bridge-run",
    "chunk-read",
    "connection-consent-denied",
    "connection-consent-warning",
    "encoder-finalize",
    "encoder-start",
    "finalizing",
    "handshake-timeout",
    "hook-drop",
    "logger-startup",
    "pipe-auth",
    "recording-cleanup",
    "recovery-scan",
    "session-failed",
    "session-start-timeout",
    "session-timeout-no-controller",
    "session-timeout-no-settings",
    "screenshot-cleanup",
    "screenshot-recovery-scan",
    "startup-recovery",
];

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecentErrorSummary {
    recent_errors: Vec<RecentErrorBucket>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecentErrorBucket {
    owner: String,
    category: String,
    phase: String,
    timestamp_bucket: String,
    recoverable: bool,
    count: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogRecordView {
    timestamp_epoch_ms: u64,
    target: String,
    phase: String,
    error: Option<LogErrorView>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogErrorView {
    code: String,
    recoverable: bool,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct RecentErrorKey {
    owner: String,
    category: String,
    phase: String,
    timestamp_bucket: String,
    recoverable: bool,
}

#[derive(Clone, Debug)]
struct RecentErrorAccumulator {
    bucket_index: u64,
    bucket: RecentErrorBucket,
}

pub(crate) fn runtime_recent_errors_json() -> Result<String, serde_json::Error> {
    recent_errors_json_from_log_dir(&default_log_directory())
}

pub(crate) fn recent_errors_json_from_log_dir(
    directory: &Path,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&recent_error_summary_from_log_dir(directory))
}

fn default_log_directory() -> std::path::PathBuf {
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

fn recent_error_summary_from_log_dir(directory: &Path) -> RecentErrorSummary {
    let mut files = managed_log_files(directory);
    files.sort();
    files.reverse();

    RecentErrorSummary {
        recent_errors: sorted_recent_error_buckets(recent_error_buckets(files)),
    }
}

fn recent_error_buckets(
    files: Vec<std::path::PathBuf>,
) -> HashMap<RecentErrorKey, RecentErrorAccumulator> {
    let mut buckets: HashMap<RecentErrorKey, RecentErrorAccumulator> = HashMap::new();
    for path in files {
        let Some(content) = read_log_file_best_effort(&path) else {
            continue;
        };
        for line in content.lines() {
            let Ok(record) = serde_json::from_str::<LogRecordView>(line) else {
                continue;
            };
            let Some(error) = record.error.as_ref() else {
                continue;
            };
            record_recent_error(&mut buckets, &record, error);
        }
    }
    buckets
}

fn record_recent_error(
    buckets: &mut HashMap<RecentErrorKey, RecentErrorAccumulator>,
    record: &LogRecordView,
    error: &LogErrorView,
) {
    let bucket_index = record.timestamp_epoch_ms / TIMESTAMP_BUCKET_MS;
    let key = RecentErrorKey {
        owner: owner_from_target(&record.target).to_owned(),
        category: allowed_label(&error.code, ALLOWED_ERROR_CATEGORIES),
        phase: allowed_label(&record.phase, ALLOWED_PHASES),
        timestamp_bucket: timestamp_bucket(bucket_index),
        recoverable: error.recoverable,
    };
    buckets
        .entry(key.clone())
        .and_modify(|entry| entry.bucket.count = entry.bucket.count.saturating_add(1))
        .or_insert_with(|| RecentErrorAccumulator {
            bucket_index,
            bucket: RecentErrorBucket {
                owner: key.owner,
                category: key.category,
                phase: key.phase,
                timestamp_bucket: key.timestamp_bucket,
                recoverable: key.recoverable,
                count: 1,
            },
        });
}

fn sorted_recent_error_buckets(
    buckets: HashMap<RecentErrorKey, RecentErrorAccumulator>,
) -> Vec<RecentErrorBucket> {
    let mut recent_errors: Vec<RecentErrorAccumulator> = buckets.into_values().collect();
    recent_errors.sort_by(|left, right| {
        right
            .bucket_index
            .cmp(&left.bucket_index)
            .then_with(|| left.bucket.owner.cmp(&right.bucket.owner))
            .then_with(|| left.bucket.category.cmp(&right.bucket.category))
            .then_with(|| left.bucket.phase.cmp(&right.bucket.phase))
    });
    recent_errors
        .into_iter()
        .take(MAX_RECENT_ERRORS)
        .map(|entry| entry.bucket)
        .collect()
}

fn read_log_file_best_effort(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn managed_log_files(directory: &Path) -> Vec<std::path::PathBuf> {
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

fn owner_from_target(target: &str) -> &'static str {
    let lower = target.to_ascii_lowercase();
    if lower.contains("diagnostics") || lower.contains("logger") {
        "diagnostics"
    } else if lower.contains("transfer") {
        "transfer"
    } else if lower.contains("telemetry") {
        "telemetry"
    } else if lower.contains("recording") || lower.contains("encoder") {
        "recording"
    } else if lower.contains("capture") {
        "capture"
    } else if lower.contains("ipc") || lower.contains("pipe") || lower.contains("native_host") {
        "ipc"
    } else if lower.contains("settings") {
        "settings"
    } else if lower.contains("tray") {
        "tray"
    } else {
        "app"
    }
}

fn timestamp_bucket(bucket_index: u64) -> String {
    format!("epoch-hour:{bucket_index}")
}

fn allowed_label(value: &str, allowed: &[&str]) -> String {
    if allowed.contains(&value) {
        value.to_owned()
    } else {
        "unknown".to_owned()
    }
}
