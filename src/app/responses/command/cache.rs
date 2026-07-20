//! Duplicate command response cache.

use super::{failure::CommandFailure, types::CommandFailureInputs};

const MAX_COMMAND_RESULTS: usize = 64;

#[derive(Clone, Debug, Default, PartialEq)]
pub(in crate::app::responses) struct CommandResultCache {
    entries: Vec<CommandResult>,
}

#[derive(Clone, Debug, PartialEq)]
struct CommandResult {
    controller_lease_id: String,
    operation: crate::protocol::AcceptedOperation,
    command_id: String,
    result: CachedCommandResult,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum CachedCommandResult {
    #[cfg(any(windows, test))]
    Accepted {
        accepted_at_epoch_ms: u64,
    },
    Failure(CommandFailure),
}

impl CommandResultCache {
    pub(super) fn result(&self, inputs: &CommandFailureInputs<'_>) -> Option<CachedCommandResult> {
        self.entries
            .iter()
            .find(|entry| {
                entry.controller_lease_id == inputs.controller_lease_id
                    && entry.operation == inputs.operation
                    && entry.command_id == inputs.command_id
            })
            .map(|entry| entry.result.clone())
    }

    pub(super) fn insert(
        &mut self,
        inputs: &CommandFailureInputs<'_>,
        result: Option<CachedCommandResult>,
    ) {
        let Some(result) = result else {
            return;
        };
        if self.entries.len() == MAX_COMMAND_RESULTS {
            self.entries.remove(0);
        }
        self.entries.push(CommandResult {
            controller_lease_id: inputs.controller_lease_id.to_owned(),
            operation: inputs.operation,
            command_id: inputs.command_id.to_owned(),
            result,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::responses::command::failure,
        protocol::{AcceptedOperation, OperationalErrorKind},
    };

    #[test]
    fn command_result_cache_keeps_typed_accept_or_failure_metadata_only() {
        let mut cache = CommandResultCache::default();
        let inputs = CommandFailureInputs {
            controller_lease_id: "lease:connection-1",
            operation: AcceptedOperation::Screenshot,
            command_id: "command-1",
            settings_revision: None,
            requested_at_epoch_ms: 4_000,
        };

        cache.insert(
            &inputs,
            Some(CachedCommandResult::Accepted {
                accepted_at_epoch_ms: 4_000,
            }),
        );

        assert_eq!(
            cache.result(&inputs),
            Some(CachedCommandResult::Accepted {
                accepted_at_epoch_ms: 4_000
            })
        );
        assert_eq!(
            failure::unsupported_capability(&inputs).kind(),
            OperationalErrorKind::UnsupportedCapability
        );
    }
}
