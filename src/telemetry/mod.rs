//! Recording-only hooks, redaction, and media timebase conversion.

#[cfg(any(windows, test))]
mod collector;
#[cfg(any(windows, test))]
mod hook_queue;
#[cfg(any(windows, test))]
mod hooks;
mod policy;
#[cfg(any(windows, test))]
mod runtime;
#[cfg(any(windows, test))]
mod sanitize;
#[cfg(any(windows, test))]
mod snapshot;
#[cfg(test)]
mod tests;
#[cfg(any(windows, test))]
mod timebase;
#[cfg(any(windows, test))]
mod windows;

#[cfg(any(windows, test))]
pub(crate) use collector::RecordingTelemetryCollector;
#[cfg(test)]
pub(crate) use collector::TelemetryCollectorError;
#[cfg(test)]
pub(crate) use hook_queue::{TelemetryHookEventQueue, TelemetryHookQueueSnapshot};
pub(crate) use policy::RecordingTelemetryPolicy;
#[cfg(windows)]
pub(crate) use runtime::default_recording_telemetry_source;
#[cfg(test)]
pub(crate) use sanitize::{KeyCode, KeyModifiers, sanitize_key};
#[cfg(any(windows, test))]
pub(crate) use snapshot::{
    NoopRecordingTelemetrySource, RecordingTelemetrySource, SanitizedRecordingTelemetrySnapshot,
};
#[cfg(test)]
pub(crate) use snapshot::{RecordingTelemetryActionEvent, RecordingTelemetrySignal};
