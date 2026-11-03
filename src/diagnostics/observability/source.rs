//! Managed process-log source for observability snapshots.

use std::path::Path;

use serde::Deserialize;

use crate::diagnostics::log::source::{managed_log_files, read_log_file_best_effort};

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LogRecordView {
    pub(super) timestamp_epoch_ms: u64,
    pub(super) target: String,
    pub(super) operation: String,
    pub(super) phase: String,
    pub(super) message: String,
    pub(super) error: Option<LogErrorView>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LogErrorView {
    pub(super) code: String,
    pub(super) recoverable: bool,
}

pub(super) fn log_records(directory: &Path) -> Vec<LogRecordView> {
    let mut records = Vec::new();
    for path in managed_log_files(directory) {
        let Some(content) = read_log_file_best_effort(&path) else {
            continue;
        };
        records.extend(
            content
                .lines()
                .filter_map(|line| serde_json::from_str::<LogRecordView>(line).ok()),
        );
    }
    records
}
