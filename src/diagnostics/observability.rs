//! Redaction-safe observability schema and runtime snapshot.

mod snapshot;
mod source;

use serde::Serialize;

#[cfg(test)]
pub(crate) use snapshot::observability_snapshot_from_log_dir;
pub(crate) use snapshot::{runtime_observability_json, runtime_observability_snapshot};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ObservabilityMetricKind {
    Counter,
    Event,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ObservabilityMetric {
    pub(crate) name: &'static str,
    pub(crate) kind: ObservabilityMetricKind,
    pub(crate) owner: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ObservabilitySnapshot {
    pub(crate) counters: Vec<ObservabilityCounter>,
    pub(crate) events: Vec<ObservabilityEvent>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ObservabilityCounter {
    pub(crate) owner: &'static str,
    pub(crate) category: &'static str,
    pub(crate) operation: &'static str,
    pub(crate) phase: &'static str,
    pub(crate) timestamp_bucket: String,
    pub(crate) count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ObservabilityEvent {
    pub(crate) owner: &'static str,
    pub(crate) category: &'static str,
    pub(crate) operation: &'static str,
    pub(crate) phase: &'static str,
    pub(crate) timestamp_bucket: String,
    pub(crate) status: &'static str,
    pub(crate) recoverable: bool,
}

pub(crate) const OBSERVABILITY_METRICS: &[ObservabilityMetric] = &[
    counter("pipe.sessions.active", "ipc"),
    counter("pipe.session_start.failures", "ipc"),
    counter("pipe.auth.failures", "ipc"),
    counter("worker.command_queue.backpressure", "app"),
    counter("transfer.pending", "transfer"),
    counter("transfer.finalized", "transfer"),
    counter("transfer.rejected", "transfer"),
    counter("transfer.chunk_read.failures", "transfer"),
    counter("transfer.cleanup.deletes", "transfer"),
    counter("telemetry.hook.drops", "telemetry"),
    counter("capture.retries", "capture"),
    counter("capture.cancelled", "capture"),
    counter("encoder.start.failures", "recording"),
    counter("encoder.finalize.failures", "recording"),
    event("logger.init.status", "diagnostics"),
];

#[cfg(test)]
pub(crate) const OBSERVABILITY_ALLOWED_FIELDS: &[&str] = &[
    "category",
    "count",
    "duration_ms",
    "operation",
    "owner",
    "phase",
    "recoverable",
    "status",
    "timestamp_bucket",
];

#[cfg(test)]
pub(crate) const OBSERVABILITY_FORBIDDEN_FIELDS: &[&str] = &[
    "auth_header",
    "base64",
    "clipboard",
    "controller_lease_id",
    "extension_id",
    "extension_origin",
    "full_path",
    "media_bytes",
    "media_id",
    "raw_key",
    "raw_payload",
    "token",
    "typed_text",
    "url",
];

const fn counter(name: &'static str, owner: &'static str) -> ObservabilityMetric {
    ObservabilityMetric {
        name,
        kind: ObservabilityMetricKind::Counter,
        owner,
    }
}

const fn event(name: &'static str, owner: &'static str) -> ObservabilityMetric {
    ObservabilityMetric {
        name,
        kind: ObservabilityMetricKind::Event,
        owner,
    }
}

fn timestamp_bucket(bucket_index: u64) -> String {
    format!("epoch-hour:{bucket_index}")
}

fn operation_for_metric(category: &'static str) -> &'static str {
    category
        .split_once('.')
        .map_or(category, |(owner, _)| owner)
}

fn phase_for_metric(category: &str) -> &'static str {
    if category.contains("failures") {
        "failure"
    } else if category.contains("status") {
        "status"
    } else {
        "runtime"
    }
}

#[cfg(any(all(windows, not(test)), test))]
pub(crate) fn logger_startup_warning_line(
    process_mode: &'static str,
    error_code: &'static str,
) -> String {
    format!(
        "logger startup warning category=logger.init.status status=failure mode={process_mode} code={error_code}\n"
    )
}

#[cfg(test)]
mod tests {
    use std::{error::Error, fs};

    use super::{
        OBSERVABILITY_ALLOWED_FIELDS, OBSERVABILITY_FORBIDDEN_FIELDS, OBSERVABILITY_METRICS,
        ObservabilityMetricKind, observability_snapshot_from_log_dir,
    };

    #[test]
    fn schema_covers_wave1_metric_families() {
        for required in [
            "pipe.sessions.active",
            "pipe.session_start.failures",
            "pipe.auth.failures",
            "worker.command_queue.backpressure",
            "transfer.pending",
            "transfer.finalized",
            "transfer.rejected",
            "transfer.chunk_read.failures",
            "transfer.cleanup.deletes",
            "telemetry.hook.drops",
            "capture.retries",
            "capture.cancelled",
            "encoder.start.failures",
            "encoder.finalize.failures",
            "logger.init.status",
        ] {
            assert!(
                OBSERVABILITY_METRICS
                    .iter()
                    .any(|metric| metric.name == required),
                "missing metric {required}"
            );
        }
        assert!(
            OBSERVABILITY_METRICS
                .iter()
                .any(|metric| metric.kind == ObservabilityMetricKind::Event)
        );
    }

    #[test]
    fn schema_fields_are_redaction_safe() {
        for forbidden in OBSERVABILITY_FORBIDDEN_FIELDS {
            assert!(
                !OBSERVABILITY_ALLOWED_FIELDS.contains(forbidden),
                "forbidden observability field was allowlisted: {forbidden}"
            );
        }
        for metric in OBSERVABILITY_METRICS {
            assert!(!metric.name.contains("payload"));
            assert!(!metric.name.contains("media_id"));
            assert!(!metric.name.contains("url"));
            assert!(!metric.name.contains("token"));
        }
    }

    #[test]
    fn snapshot_reports_all_metric_families_without_sensitive_values() -> Result<(), Box<dyn Error>>
    {
        let directory = tempfile::tempdir()?;
        write_observability_fixture(directory.path())?;

        let snapshot = observability_snapshot_from_log_dir(directory.path());
        let output = serde_json::to_string(&snapshot)?;
        for metric in OBSERVABILITY_METRICS {
            assert!(output.contains(metric.name), "missing {}", metric.name);
        }
        assert_snapshot_counts(&snapshot);
        assert_no_sensitive_observability_values(&output);
        Ok(())
    }

    fn write_observability_fixture(directory: &std::path::Path) -> Result<(), std::io::Error> {
        fs::write(
            directory.join("sniptale-native-2000-77-0.jsonl"),
            r#"{"timestampEpochMs":3600000,"target":"sniptale_native_agent::resident_pipe","operation":"native-host-session","phase":"session-accepted","message":"chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"}
{"timestampEpochMs":3600100,"target":"sniptale_native_agent::resident_pipe","operation":"native-host-session","phase":"session-start-timeout","error":{"code":"resident_pipe.session_start_timeout","recoverable":true},"message":"C:\\Users\\Alice\\Private"}
{"timestampEpochMs":3600200,"target":"sniptale_native_agent::resident_pipe","operation":"native-host-session","phase":"handshake-timeout","error":{"code":"resident_pipe.handshake_timeout","recoverable":true},"message":"token secret-token"}
{"timestampEpochMs":3600300,"target":"sniptale_native_agent::transfer_worker","operation":"transfer-cleanup","phase":"recording-cleanup","message":"recording transfer cleanup summary pending=2 retainedRejected=1 deleted=3 finalized=4"}
{"timestampEpochMs":3600400,"target":"sniptale_native_agent::diagnostics","operation":"startup","phase":"logging","message":"process logging initialized"}
{"timestampEpochMs":3600500,"target":"sniptale_native_agent::telemetry","operation":"telemetry","phase":"hook-drop","error":{"code":"telemetry.hook_drop","recoverable":true},"message":"typed text: password123"}
{"timestampEpochMs":7200000,"target":"sniptale_native_agent::diagnostics","operation":"logging","phase":"logger-startup","error":{"code":"logger.file","recoverable":true},"message":"C:\\Users\\Alice\\Private"}
"#,
        )
    }

    fn assert_snapshot_counts(snapshot: &super::ObservabilitySnapshot) {
        assert!(
            snapshot.counters.iter().any(|counter| {
                counter.category == "pipe.sessions.active" && counter.count == 1
            })
        );
        assert!(snapshot.counters.iter().any(|counter| {
            counter.category == "pipe.session_start.failures" && counter.count == 1
        }));
        assert!(
            snapshot
                .counters
                .iter()
                .any(|counter| { counter.category == "pipe.auth.failures" && counter.count == 1 })
        );
        assert!(snapshot.counters.iter().any(|counter| {
            counter.category == "transfer.cleanup.deletes" && counter.count == 1
        }));
        assert!(
            snapshot.counters.iter().any(|counter| {
                counter.category == "telemetry.hook.drops" && counter.count == 1
            })
        );
        assert!(
            snapshot.events.iter().any(|event| {
                event.category == "logger.init.status" && event.status == "success"
            })
        );
        assert!(
            snapshot.events.iter().any(|event| {
                event.category == "logger.init.status" && event.status == "failure"
            })
        );
    }

    fn assert_no_sensitive_observability_values(output: &str) {
        assert!(!output.contains("C:\\Users\\Alice"));
        assert!(!output.contains("secret-token"));
        assert!(!output.contains("password123"));
        assert!(!output.contains("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
        assert!(!output.contains("chrome-extension://"));
    }

    #[test]
    fn logger_startup_warning_line_is_category_only() {
        let line = super::logger_startup_warning_line("native-host", "logger.file");

        assert!(line.contains("category=logger.init.status"));
        assert!(line.contains("status=failure"));
        assert!(line.contains("mode=native-host"));
        assert!(line.contains("code=logger.file"));
        assert!(!line.contains(":\\Users\\"));
        assert!(!line.contains("chrome-extension://"));
        assert!(!line.contains("token"));
    }
}
