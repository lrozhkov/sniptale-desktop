//! Recording telemetry signal serialization.

use serde_json::Value;
#[cfg(any(windows, test))]
use serde_json::json;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RecordingTelemetrySignal {
    #[cfg(any(windows, test))]
    TypingSpan {
        count: u16,
        start_time_ms: u64,
        end_time_ms: u64,
    },
    #[cfg(any(windows, test))]
    CursorIdle {
        start_time_ms: u64,
        end_time_ms: u64,
        x: i32,
        y: i32,
    },
    #[cfg(any(windows, test))]
    StaticFrame {
        start_time_ms: u64,
        end_time_ms: u64,
        source: &'static str,
        code: &'static str,
        severity: &'static str,
    },
}

impl RecordingTelemetrySignal {
    #[cfg(any(windows, test))]
    pub(crate) fn typing_span(count: u16, start_time_ms: u64, end_time_ms: u64) -> Self {
        Self::TypingSpan {
            count,
            start_time_ms,
            end_time_ms,
        }
    }

    #[cfg(any(windows, test))]
    pub(crate) const fn cursor_idle(start_time_ms: u64, end_time_ms: u64, x: i32, y: i32) -> Self {
        Self::CursorIdle {
            start_time_ms,
            end_time_ms,
            x,
            y,
        }
    }

    #[cfg(any(windows, test))]
    pub(crate) const fn telemetry_warning(
        source: &'static str,
        code: &'static str,
        time_ms: u64,
    ) -> Self {
        Self::StaticFrame {
            start_time_ms: time_ms,
            end_time_ms: time_ms,
            source,
            code,
            severity: "warning",
        }
    }

    pub(super) fn into_protocol(self, index: usize) -> Value {
        #[cfg(not(any(windows, test)))]
        let _ = index;
        match self {
            #[cfg(any(windows, test))]
            Self::TypingSpan {
                count,
                start_time_ms,
                end_time_ms,
            } => typing_signal_protocol(index, count, start_time_ms, end_time_ms),
            #[cfg(any(windows, test))]
            Self::CursorIdle {
                start_time_ms,
                end_time_ms,
                x,
                y,
            } => cursor_idle_signal_protocol(index, start_time_ms, end_time_ms, x, y),
            #[cfg(any(windows, test))]
            Self::StaticFrame {
                start_time_ms,
                end_time_ms,
                source,
                code,
                severity,
            } => static_frame_signal_protocol(
                index,
                start_time_ms,
                end_time_ms,
                source,
                code,
                severity,
            ),
        }
    }
}

#[cfg(any(windows, test))]
fn typing_signal_protocol(index: usize, count: u16, start_time_ms: u64, end_time_ms: u64) -> Value {
    json!({
        "id": format!("native-typing-{index}"),
        "kind": "typing",
        "startTime": start_time_ms,
        "endTime": end_time_ms,
        "point": null,
        "data": {
            "count": count
        }
    })
}

#[cfg(any(windows, test))]
fn cursor_idle_signal_protocol(
    index: usize,
    start_time_ms: u64,
    end_time_ms: u64,
    x: i32,
    y: i32,
) -> Value {
    json!({
        "id": format!("native-cursor-idle-{index}"),
        "kind": "cursor-idle",
        "startTime": start_time_ms,
        "endTime": end_time_ms,
        "point": {
            "x": x,
            "y": y
        },
        "data": {
            "source": "native-cursor"
        }
    })
}

#[cfg(any(windows, test))]
fn static_frame_signal_protocol(
    index: usize,
    start_time_ms: u64,
    end_time_ms: u64,
    source: &'static str,
    code: &'static str,
    severity: &'static str,
) -> Value {
    json!({
        "id": format!("native-static-{index}"),
        "kind": "static-frame",
        "startTime": start_time_ms,
        "endTime": end_time_ms,
        "point": null,
        "data": {
            "source": source,
            "code": code,
            "severity": severity
        }
    })
}
