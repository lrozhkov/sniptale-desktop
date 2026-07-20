//! Command response wire builders.

use serde_json::Value;

#[cfg(test)]
use crate::app::responses::NativeHostCommandEvent;
#[cfg(any(windows, test))]
use crate::app::responses::outbound::command_accepted;
#[cfg(test)]
use crate::app::responses::outbound::command_event;
use crate::app::responses::outbound::{OperationFailure, operation_failed};

use super::{
    cache::CachedCommandResult,
    failure::{self, CommandFailure},
    types::CommandFailureInputs,
};

pub(super) fn cached_result(
    inputs: &CommandFailureInputs<'_>,
    result: CachedCommandResult,
) -> Vec<Value> {
    match result {
        #[cfg(any(windows, test))]
        CachedCommandResult::Accepted {
            accepted_at_epoch_ms,
        } => vec![accepted(inputs, accepted_at_epoch_ms)],
        CachedCommandResult::Failure(failure) => vec![failure_response(inputs, &failure)],
    }
}

#[cfg(test)]
pub(super) fn accepted_with_events(
    inputs: &CommandFailureInputs<'_>,
    events: Vec<NativeHostCommandEvent>,
) -> CommandResultValues {
    let mut values = Vec::with_capacity(events.len().saturating_add(1));
    let accepted = accepted(inputs, inputs.requested_at_epoch_ms);
    values.push(accepted);
    values.extend(events.into_iter().map(command_event));
    CommandResultValues {
        values,
        cached: Some(CachedCommandResult::Accepted {
            accepted_at_epoch_ms: inputs.requested_at_epoch_ms,
        }),
    }
}

#[cfg(any(windows, test))]
pub(super) fn accepted_result(inputs: &CommandFailureInputs<'_>) -> CommandResultValues {
    CommandResultValues {
        values: vec![accepted(inputs, inputs.requested_at_epoch_ms)],
        cached: Some(CachedCommandResult::Accepted {
            accepted_at_epoch_ms: inputs.requested_at_epoch_ms,
        }),
    }
}

pub(super) fn failure_result(
    inputs: &CommandFailureInputs<'_>,
    failure: CommandFailure,
) -> CommandResultValues {
    CommandResultValues {
        values: vec![failure_response(inputs, &failure)],
        cached: Some(CachedCommandResult::Failure(failure)),
    }
}

pub(super) fn uncached_failure(
    inputs: &CommandFailureInputs<'_>,
    failure: &CommandFailure,
) -> Vec<Value> {
    vec![failure_response(inputs, failure)]
}

#[cfg(any(windows, test))]
fn accepted(inputs: &CommandFailureInputs<'_>, accepted_at_epoch_ms: u64) -> Value {
    command_accepted(
        inputs.controller_lease_id,
        inputs.command_id,
        inputs.operation,
        accepted_at_epoch_ms,
    )
}

fn failure_response(inputs: &CommandFailureInputs<'_>, failure: &CommandFailure) -> Value {
    operation_failed(OperationFailure {
        controller_lease_id: Some(inputs.controller_lease_id),
        operation: failure::command_operation(inputs.operation),
        operation_id: Some(inputs.command_id),
        phase: failure.phase(),
        kind: failure.kind(),
        message: failure.message(),
        occurred_at_epoch_ms: failure.occurred_at_epoch_ms(),
    })
}

pub(super) struct CommandResultValues {
    pub(super) values: Vec<Value>,
    pub(super) cached: Option<CachedCommandResult>,
}
