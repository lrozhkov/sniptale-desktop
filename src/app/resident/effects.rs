//! Resident agent side-effect adapter for authenticated pipe sessions.

mod agent;
mod media_dispatch;
mod transfer;
mod worker_sinks;

pub(super) use agent::AgentSideEffects;

#[cfg(test)]
#[path = "effects/mode_gate.rs"]
mod mode_gate;
#[cfg(test)]
mod tests;
