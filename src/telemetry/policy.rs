//! Recording telemetry collection policy.

#[cfg(any(windows, test))]
use super::hooks::TelemetryHookKind;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct RecordingTelemetryPolicy {
    pub(crate) collect_cursor: bool,
    pub(crate) collect_clicks: bool,
    pub(crate) collect_key_events: bool,
    pub(crate) collect_typing_spans: bool,
    pub(crate) collect_static_signals: bool,
    pub(crate) telemetry_non_fatal: bool,
}

impl Default for RecordingTelemetryPolicy {
    fn default() -> Self {
        Self {
            collect_cursor: false,
            collect_clicks: false,
            collect_key_events: false,
            collect_typing_spans: false,
            collect_static_signals: false,
            telemetry_non_fatal: true,
        }
    }
}

impl RecordingTelemetryPolicy {
    #[cfg(test)]
    pub(crate) const fn all_enabled_for_tests() -> Self {
        Self {
            collect_cursor: true,
            collect_clicks: true,
            collect_key_events: true,
            collect_typing_spans: true,
            collect_static_signals: true,
            telemetry_non_fatal: true,
        }
    }

    #[cfg(any(windows, test))]
    pub(crate) fn requested_hook_kinds(self) -> Vec<TelemetryHookKind> {
        let mut hooks = Vec::new();
        if self.collect_key_events || self.collect_typing_spans {
            hooks.push(TelemetryHookKind::Keyboard);
        }
        if self.collect_cursor || self.collect_clicks || self.collect_static_signals {
            hooks.push(TelemetryHookKind::Mouse);
        }
        hooks
    }
}
