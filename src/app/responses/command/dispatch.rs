//! Command lease, duplicate, and side-effect dispatch policy.

use serde_json::Value;

use super::{
    cache::CommandResultCache, failure, lease_gate, types::CommandFailureInputs, wire,
    wire::CommandResultValues,
};
use crate::app::responses::{
    NativeHostCommandOutcome, NativeHostSideEffectError, session::ActiveLeaseValidation,
};

pub(in crate::app::responses) fn command_response(
    cache: &mut CommandResultCache,
    lease_validation: ActiveLeaseValidation,
    accepted_settings_revision: Option<&str>,
    inputs: CommandFailureInputs<'_>,
    dispatch: impl FnOnce() -> Result<NativeHostCommandOutcome, NativeHostSideEffectError>,
) -> Vec<Value> {
    if let Err(failure) = lease_gate::validate(lease_validation, &inputs) {
        return wire::uncached_failure(&inputs, &failure);
    }
    if let Some(result) = cache.result(&inputs) {
        return wire::cached_result(&inputs, result);
    }
    let result = command_result(accepted_settings_revision, &inputs, dispatch);
    cache.insert(&inputs, result.cached.clone());
    result.values
}

fn command_result(
    accepted_settings_revision: Option<&str>,
    inputs: &CommandFailureInputs<'_>,
    dispatch: impl FnOnce() -> Result<NativeHostCommandOutcome, NativeHostSideEffectError>,
) -> CommandResultValues {
    if inputs
        .settings_revision
        .is_some_and(|revision| accepted_settings_revision != Some(revision))
    {
        return wire::failure_result(inputs, failure::settings_revision(inputs));
    }
    match dispatch() {
        #[cfg(any(windows, test))]
        Ok(NativeHostCommandOutcome::Accepted) => wire::accepted_result(inputs),
        #[cfg(test)]
        Ok(NativeHostCommandOutcome::AcceptedWithEvents(events)) => {
            wire::accepted_with_events(inputs, events)
        }
        Ok(NativeHostCommandOutcome::UnsupportedCapability) => {
            wire::failure_result(inputs, failure::unsupported_capability(inputs))
        }
        #[cfg(any(windows, test))]
        Err(NativeHostSideEffectError::UnsupportedCapability) => {
            wire::failure_result(inputs, failure::unsupported_capability(inputs))
        }
        #[cfg(test)]
        Err(NativeHostSideEffectError::RecordingDiskPressure) => {
            wire::failure_result(inputs, failure::disk_pressure(inputs))
        }
        #[cfg(any(windows, test))]
        Err(NativeHostSideEffectError::WorkerQueueFull) => {
            wire::failure_result(inputs, failure::queue_backpressure(inputs))
        }
        #[cfg(any(windows, test))]
        Err(NativeHostSideEffectError::WorkerStopped) => {
            wire::failure_result(inputs, failure::command_dispatch(inputs))
        }
        #[cfg(test)]
        Err(NativeHostSideEffectError::ScreenshotRegionCancelled) => {
            wire::failure_result(inputs, failure::screenshot_selection_cancelled(inputs))
        }
        #[cfg(test)]
        Err(NativeHostSideEffectError::ScreenshotInvalidSelection) => {
            wire::failure_result(inputs, failure::screenshot_invalid_selection(inputs))
        }
        #[cfg(any(windows, test))]
        Err(
            NativeHostSideEffectError::ScreenshotTransferState
            | NativeHostSideEffectError::RecordingTransferState
            | NativeHostSideEffectError::RecordingRecovery,
        ) => wire::failure_result(inputs, failure::storage_unavailable(inputs)),
        #[cfg(test)]
        Err(NativeHostSideEffectError::CommandDispatch) => {
            wire::failure_result(inputs, failure::command_dispatch(inputs))
        }
        #[cfg(any(windows, test))]
        Err(NativeHostSideEffectError::HotkeyApply | NativeHostSideEffectError::TrayApply) => {
            wire::failure_result(inputs, failure::command_dispatch(inputs))
        }
        #[cfg(not(any(windows, test)))]
        Err(_) => wire::failure_result(inputs, failure::command_dispatch(inputs)),
    }
}
