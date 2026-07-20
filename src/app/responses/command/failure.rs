//! Command failure mapping and typed failure descriptors.

use crate::protocol::{AcceptedOperation, OperationFailedOperation, OperationalErrorKind};

use super::types::CommandFailureInputs;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CommandFailure {
    phase: &'static str,
    kind: OperationalErrorKind,
    message: Option<String>,
    occurred_at_epoch_ms: u64,
}

impl CommandFailure {
    fn new(
        phase: &'static str,
        kind: OperationalErrorKind,
        message: Option<&str>,
        occurred_at_epoch_ms: u64,
    ) -> Self {
        Self {
            phase,
            kind,
            message: message.map(str::to_owned),
            occurred_at_epoch_ms,
        }
    }

    pub(super) fn phase(&self) -> &'static str {
        self.phase
    }

    pub(super) fn kind(&self) -> OperationalErrorKind {
        self.kind
    }

    pub(super) fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    pub(super) fn occurred_at_epoch_ms(&self) -> u64 {
        self.occurred_at_epoch_ms
    }
}

pub(super) fn stale_lease(inputs: &CommandFailureInputs<'_>) -> CommandFailure {
    command_failure(
        inputs,
        "lease-validation",
        OperationalErrorKind::StaleControllerLease,
        None,
    )
}

pub(super) fn lease_state_unavailable(inputs: &CommandFailureInputs<'_>) -> CommandFailure {
    command_failure(
        inputs,
        "lease-validation",
        OperationalErrorKind::StorageUnavailable,
        None,
    )
}

pub(super) fn settings_revision(inputs: &CommandFailureInputs<'_>) -> CommandFailure {
    command_failure(
        inputs,
        "settings-revision",
        OperationalErrorKind::CommandSettingsRevisionMismatch,
        None,
    )
}

pub(super) fn unsupported_capability(inputs: &CommandFailureInputs<'_>) -> CommandFailure {
    command_failure(
        inputs,
        command_phase(inputs.operation),
        OperationalErrorKind::UnsupportedCapability,
        None,
    )
}

#[cfg(test)]
pub(super) fn disk_pressure(inputs: &CommandFailureInputs<'_>) -> CommandFailure {
    command_failure(
        inputs,
        "recording",
        OperationalErrorKind::DiskPressure,
        None,
    )
}

pub(super) fn command_dispatch(inputs: &CommandFailureInputs<'_>) -> CommandFailure {
    command_failure(
        inputs,
        command_phase(inputs.operation),
        OperationalErrorKind::CommandDispatchFailed,
        None,
    )
}

#[cfg(any(windows, test))]
pub(super) fn queue_backpressure(inputs: &CommandFailureInputs<'_>) -> CommandFailure {
    command_failure(
        inputs,
        command_phase(inputs.operation),
        OperationalErrorKind::QueueBackpressure,
        None,
    )
}

#[cfg(test)]
pub(super) fn screenshot_selection_cancelled(inputs: &CommandFailureInputs<'_>) -> CommandFailure {
    command_failure(
        inputs,
        "screenshot",
        OperationalErrorKind::ScreenshotSelectionCancelled,
        None,
    )
}

#[cfg(test)]
pub(super) fn screenshot_invalid_selection(inputs: &CommandFailureInputs<'_>) -> CommandFailure {
    command_failure(
        inputs,
        "screenshot",
        OperationalErrorKind::ScreenshotInvalidSelection,
        None,
    )
}

#[cfg(any(windows, test))]
pub(super) fn storage_unavailable(inputs: &CommandFailureInputs<'_>) -> CommandFailure {
    command_failure(
        inputs,
        command_phase(inputs.operation),
        OperationalErrorKind::StorageUnavailable,
        None,
    )
}

fn command_failure(
    inputs: &CommandFailureInputs<'_>,
    phase: &'static str,
    kind: OperationalErrorKind,
    message: Option<&str>,
) -> CommandFailure {
    CommandFailure::new(phase, kind, message, inputs.requested_at_epoch_ms)
}

pub(super) const fn command_operation(operation: AcceptedOperation) -> OperationFailedOperation {
    match operation {
        AcceptedOperation::Screenshot => OperationFailedOperation::Screenshot,
        AcceptedOperation::Recording => OperationFailedOperation::Recording,
    }
}

pub(super) const fn command_phase(operation: AcceptedOperation) -> &'static str {
    match operation {
        AcceptedOperation::Screenshot => "screenshot",
        AcceptedOperation::Recording => "recording",
    }
}
