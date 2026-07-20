//! Command lease gate.

use super::{failure, failure::CommandFailure, types::CommandFailureInputs};
use crate::app::responses::session::ActiveLeaseValidation;

pub(super) fn validate(
    validation: ActiveLeaseValidation,
    inputs: &CommandFailureInputs<'_>,
) -> Result<(), CommandFailure> {
    match validation {
        ActiveLeaseValidation::Valid => Ok(()),
        ActiveLeaseValidation::Invalid => Err(failure::stale_lease(inputs)),
        ActiveLeaseValidation::StateUnavailable => Err(failure::lease_state_unavailable(inputs)),
    }
}
