//! Recording-only hooks, redaction, and media timebase conversion.

#[cfg(any(windows, test))]
mod collector;
#[cfg(any(windows, test))]
mod frame;
#[cfg(any(windows, test))]
mod retained;
#[cfg(any(windows, test))]
pub(crate) use frame::{RecordingFrameGeometry, RecordingPointMapping};
#[cfg(any(windows, test))]
pub(crate) use retained::legacy_cursor_trajectory_warning;
#[cfg(any(windows, test))]
pub(crate) use retained::legacy_recording_events_warning;
#[cfg(any(windows, test))]
pub(crate) use retained::{browser_delivery_telemetry, valid_retained_telemetry};
#[cfg(any(windows, test))]
mod events;
#[cfg(test)]
pub(crate) use events::NativeRecordingEvents;
#[cfg(any(windows, test))]
mod events_clock;
#[cfg(any(windows, test))]
mod events_sink;
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
mod trajectory;
#[cfg(any(windows, test))]
mod trajectory_clock;
#[cfg(any(windows, test))]
mod trajectory_producer;
#[cfg(test)]
pub(crate) use trajectory_producer::CursorTrajectoryProducer;
#[cfg(any(windows, test))]
mod trajectory_sink;
#[cfg(any(windows, test))]
mod trajectory_window;
#[cfg(any(windows, test))]
mod windows;

#[cfg(any(windows, test))]
pub(crate) use collector::RecordingTelemetryCollector;
#[cfg(test)]
pub(crate) use collector::TelemetryCollectorError;
#[cfg(any(windows, test))]
pub(crate) use events_sink::{RecordingEventsSink, RecordingEventsSinkFactory};
#[cfg(test)]
pub(crate) use hook_queue::{TelemetryHookEventQueue, TelemetryHookQueueSnapshot};
pub(crate) use policy::RecordingTelemetryPolicy;
#[cfg(test)]
pub(crate) use runtime::RuntimeRecordingTelemetrySource;
#[cfg(windows)]
pub(crate) use runtime::default_recording_telemetry_source;
#[cfg(test)]
pub(crate) use runtime::{
    WindowsKeyEvent, WindowsKeyModifiers, WindowsKeyObservation, WindowsKeyPhase,
};
#[cfg(test)]
pub(crate) use runtime::{WindowsMouseButton, WindowsMouseEvent, WindowsMouseEventKind};
#[cfg(test)]
pub(crate) use sanitize::{KeyCode, KeyModifiers, sanitize_key};
#[cfg(any(windows, test))]
pub(crate) use snapshot::{
    NoopRecordingTelemetrySource, RecordingTelemetrySource, SanitizedRecordingTelemetrySnapshot,
};
#[cfg(test)]
pub(crate) use snapshot::{RecordingTelemetryActionEvent, RecordingTelemetrySignal};
#[cfg(any(windows, test))]
pub(crate) use trajectory_sink::{
    CursorTrajectorySink, CursorTrajectorySinkError, CursorTrajectorySinkFactory,
    RecordingTrajectoryContext,
};

#[cfg(test)]
pub(crate) use runtime::{WindowsInputObservation, merge_input_observations};
