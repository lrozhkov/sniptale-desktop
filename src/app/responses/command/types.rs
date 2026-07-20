//! Command response input descriptors.

use crate::protocol::AcceptedOperation;

#[derive(Clone, Copy)]
pub(in crate::app::responses) struct CommandFailureInputs<'a> {
    pub(in crate::app::responses) controller_lease_id: &'a str,
    pub(in crate::app::responses) operation: AcceptedOperation,
    pub(in crate::app::responses) command_id: &'a str,
    pub(in crate::app::responses) settings_revision: Option<&'a str>,
    pub(in crate::app::responses) requested_at_epoch_ms: u64,
}
