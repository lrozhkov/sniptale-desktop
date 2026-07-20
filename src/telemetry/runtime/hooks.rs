//! Runtime telemetry hook planning and installation.
#![cfg(windows)]

use super::RuntimeRecordingTelemetrySource;
use crate::telemetry::{
    hooks::{
        HookAvailability, HookAvailabilitySnapshot, HookFailure, HookPolicy, TelemetryHookKind,
        plan_requested_recording_hooks,
    },
    windows::{WindowsKeyboardTelemetryHook, WindowsMouseTelemetryHook},
};

impl RuntimeRecordingTelemetrySource {
    pub(super) fn install_runtime_hooks(&mut self, recording_id: &str, monotonic_ns: u64) {
        let requested_hooks = self.policy.requested_hook_kinds();
        let hook_policy = HookPolicy {
            telemetry_non_fatal: self.policy.telemetry_non_fatal,
        };
        let plan = match plan_requested_recording_hooks(
            &requested_hooks,
            HookAvailabilitySnapshot {
                keyboard: HookAvailability::Available,
                mouse: HookAvailability::Available,
            },
            hook_policy,
        ) {
            Ok(plan) => plan,
            Err(failure) => {
                self.record_hook_warning(recording_id, failure, monotonic_ns);
                return;
            }
        };
        for warning in plan.warnings {
            self.record_hook_warning(recording_id, warning, monotonic_ns);
        }
        for hook in plan.install {
            match hook {
                TelemetryHookKind::Keyboard => match WindowsKeyboardTelemetryHook::install() {
                    Ok(hook) => self.keyboard_hook = Some(hook),
                    Err(_) => self.record_hook_warning(
                        recording_id,
                        HookFailure::Blocked(TelemetryHookKind::Keyboard),
                        monotonic_ns,
                    ),
                },
                TelemetryHookKind::Mouse => match WindowsMouseTelemetryHook::install() {
                    Ok(hook) => self.mouse_hook = Some(hook),
                    Err(_) => self.record_hook_warning(
                        recording_id,
                        HookFailure::Blocked(TelemetryHookKind::Mouse),
                        monotonic_ns,
                    ),
                },
            }
        }
    }

    fn record_hook_warning(&mut self, recording_id: &str, failure: HookFailure, monotonic_ns: u64) {
        let (source, code) = hook_failure_signal(failure);
        self.record_telemetry_warning_best_effort(recording_id, source, code, monotonic_ns);
    }
}

fn hook_failure_signal(failure: HookFailure) -> (&'static str, &'static str) {
    match failure {
        #[cfg(test)]
        HookFailure::PermissionDenied(TelemetryHookKind::Keyboard) => {
            ("keyboard-hook", "permission-denied")
        }
        #[cfg(test)]
        HookFailure::PermissionDenied(TelemetryHookKind::Mouse) => {
            ("mouse-hook", "permission-denied")
        }
        #[cfg(test)]
        HookFailure::PolicyDenied(TelemetryHookKind::Keyboard) => {
            ("keyboard-hook", "policy-denied")
        }
        #[cfg(test)]
        HookFailure::PolicyDenied(TelemetryHookKind::Mouse) => ("mouse-hook", "policy-denied"),
        HookFailure::Blocked(TelemetryHookKind::Keyboard) => ("keyboard-hook", "blocked"),
        HookFailure::Blocked(TelemetryHookKind::Mouse) => ("mouse-hook", "blocked"),
    }
}
