//! Runtime observability snapshot sourced from managed process logs.

use std::{collections::HashMap, path::Path};

use super::{
    OBSERVABILITY_METRICS, ObservabilityCounter, ObservabilityEvent, ObservabilityMetricKind,
    ObservabilitySnapshot, counter, operation_for_metric, phase_for_metric,
    source::{LogRecordView, default_log_directory, log_records},
    timestamp_bucket,
};

const TIMESTAMP_BUCKET_MS: u64 = 60 * 60 * 1_000;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct CounterKey {
    category: &'static str,
    bucket_index: u64,
}

pub(crate) fn runtime_observability_snapshot() -> ObservabilitySnapshot {
    observability_snapshot_from_log_dir(&default_log_directory())
}

pub(crate) fn runtime_observability_json() -> Result<String, serde_json::Error> {
    serde_json::to_string(&runtime_observability_snapshot())
}

pub(crate) fn observability_snapshot_from_log_dir(directory: &Path) -> ObservabilitySnapshot {
    let (mut counters, mut indexed) = default_indexed_counters();
    let mut active_sessions: isize = 0;
    let mut events = Vec::new();
    for record in log_records(directory) {
        collect_record_observability(
            &record,
            &mut active_sessions,
            &mut counters,
            &mut indexed,
            &mut events,
        );
    }
    apply_active_session_counter(active_sessions, &mut counters, &mut indexed);
    ensure_logger_event(&mut events);
    sort_observability(&mut counters, &mut events);
    ObservabilitySnapshot { counters, events }
}

fn default_indexed_counters() -> (Vec<ObservabilityCounter>, HashMap<CounterKey, usize>) {
    let counters = default_counters();
    let indexed = counters
        .iter()
        .enumerate()
        .map(|(index, counter)| {
            (
                CounterKey {
                    category: counter.category,
                    bucket_index: 0,
                },
                index,
            )
        })
        .collect();
    (counters, indexed)
}

fn collect_record_observability(
    record: &LogRecordView,
    active_sessions: &mut isize,
    counters: &mut Vec<ObservabilityCounter>,
    indexed: &mut HashMap<CounterKey, usize>,
    events: &mut Vec<ObservabilityEvent>,
) {
    let bucket_index = record.timestamp_epoch_ms / TIMESTAMP_BUCKET_MS;
    observe_session_activity(record, active_sessions);
    if let Some(category) = counter_category(record) {
        increment_counter(counters, indexed, category, bucket_index, 1);
    }
    if record.operation == "startup" && record.phase == "logging" {
        events.push(logger_init_event(record, "success"));
    } else if record.phase == "logger-startup" {
        events.push(logger_init_event(record, "failure"));
    }
}

fn observe_session_activity(record: &LogRecordView, active_sessions: &mut isize) {
    if record.phase == "session-accepted" {
        *active_sessions = active_sessions.saturating_add(1);
    }
    if matches!(
        record.phase.as_str(),
        "session-completed" | "session-failed"
    ) {
        *active_sessions = active_sessions.saturating_sub(1).max(0);
    }
}

fn apply_active_session_counter(
    active_sessions: isize,
    counters: &mut Vec<ObservabilityCounter>,
    indexed: &mut HashMap<CounterKey, usize>,
) {
    if active_sessions > 0 {
        increment_counter(
            counters,
            indexed,
            "pipe.sessions.active",
            0,
            active_sessions.cast_unsigned(),
        );
    }
}

fn ensure_logger_event(events: &mut Vec<ObservabilityEvent>) {
    if events.is_empty() {
        events.push(ObservabilityEvent {
            owner: "diagnostics",
            category: "logger.init.status",
            operation: "logging",
            phase: "logger-startup",
            timestamp_bucket: timestamp_bucket(0),
            status: "unknown",
            recoverable: true,
        });
    }
}

fn sort_observability(counters: &mut [ObservabilityCounter], events: &mut [ObservabilityEvent]) {
    counters.sort_by(|left, right| {
        left.category
            .cmp(right.category)
            .then_with(|| left.timestamp_bucket.cmp(&right.timestamp_bucket))
    });
    events.sort_by(|left, right| {
        left.category
            .cmp(right.category)
            .then_with(|| left.timestamp_bucket.cmp(&right.timestamp_bucket))
            .then_with(|| left.status.cmp(right.status))
    });
}

fn counter_category(record: &LogRecordView) -> Option<&'static str> {
    let code = record
        .error
        .as_ref()
        .map(|error| error.code.as_str())
        .unwrap_or_default();
    let phase = record.phase.as_str();
    let target = record.target.as_str();
    if phase == "session-start-timeout" || code == "resident_pipe.session_start_timeout" {
        Some("pipe.session_start.failures")
    } else if phase.contains("auth") || phase.contains("handshake") || code.contains("pipe_auth") {
        Some("pipe.auth.failures")
    } else if code.contains("queue") || code.contains("backpressure") {
        Some("worker.command_queue.backpressure")
    } else if target.contains("transfer") && code.contains("chunk") {
        Some("transfer.chunk_read.failures")
    } else if target.contains("telemetry") && (code.contains("hook") || code.contains("drop")) {
        Some("telemetry.hook.drops")
    } else if target.contains("capture") && phase.contains("retry") {
        Some("capture.retries")
    } else if target.contains("capture") && phase.contains("cancel") {
        Some("capture.cancelled")
    } else if target.contains("recording") && phase.contains("encoder-start") {
        Some("encoder.start.failures")
    } else if target.contains("recording") && phase.contains("encoder-finalize") {
        Some("encoder.finalize.failures")
    } else {
        transfer_summary_category(record)
    }
}

fn transfer_summary_category(record: &LogRecordView) -> Option<&'static str> {
    if !(record.target.contains("transfer") || record.target.contains("startup_recovery")) {
        return None;
    }
    if record.message.contains("deleted=") {
        Some("transfer.cleanup.deletes")
    } else if record.message.contains("pending=") || record.message.contains("pendingRecordings=") {
        Some("transfer.pending")
    } else if record.message.contains("retainedRejected=") {
        Some("transfer.rejected")
    } else if record.message.contains("finalized=") {
        Some("transfer.finalized")
    } else {
        None
    }
}

fn logger_init_event(record: &LogRecordView, status: &'static str) -> ObservabilityEvent {
    ObservabilityEvent {
        owner: "diagnostics",
        category: "logger.init.status",
        operation: "logging",
        phase: "logger-startup",
        timestamp_bucket: timestamp_bucket(record.timestamp_epoch_ms / TIMESTAMP_BUCKET_MS),
        status,
        recoverable: record.error.as_ref().is_none_or(|error| error.recoverable),
    }
}

fn increment_counter(
    counters: &mut Vec<ObservabilityCounter>,
    indexed: &mut HashMap<CounterKey, usize>,
    category: &'static str,
    bucket_index: u64,
    count: usize,
) {
    let key = CounterKey {
        category,
        bucket_index,
    };
    if let Some(index) = indexed.get(&key).copied() {
        counters[index].count = counters[index].count.saturating_add(count);
        return;
    }
    let index = counters.len();
    counters.push(counter_sample(category, bucket_index, count));
    indexed.insert(key, index);
}

fn default_counters() -> Vec<ObservabilityCounter> {
    OBSERVABILITY_METRICS
        .iter()
        .filter(|metric| metric.kind == ObservabilityMetricKind::Counter)
        .map(|metric| counter_sample(metric.name, 0, 0))
        .collect()
}

fn counter_sample(category: &'static str, bucket_index: u64, count: usize) -> ObservabilityCounter {
    let metric = OBSERVABILITY_METRICS
        .iter()
        .find(|metric| metric.name == category)
        .copied()
        .unwrap_or_else(|| counter(category, "diagnostics"));
    ObservabilityCounter {
        owner: metric.owner,
        category,
        operation: operation_for_metric(category),
        phase: phase_for_metric(category),
        timestamp_bucket: timestamp_bucket(bucket_index),
        count,
    }
}
