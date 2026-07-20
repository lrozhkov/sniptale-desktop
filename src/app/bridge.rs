//! Native Messaging stdio bridge lifecycle facade.

mod error;
#[cfg(any(not(windows), test))]
mod local;
mod runtime;
#[cfg(any(windows, test))]
pub(crate) mod session;
#[cfg(any(windows, test))]
mod startup;
#[cfg(all(windows, not(test)))]
mod windows_runtime;

pub(crate) use error::NativeHostRunError;
#[cfg(test)]
pub(crate) use runtime::{NativeHostPipeAuthConfig, run_authenticated_pipe_forwarding_bridge};
pub(crate) use runtime::{NativeHostRuntimeLog, run_native_host_bridge};
#[cfg(test)]
pub(crate) use startup::{
    BridgeStartupError, BridgeStartupInputs, connect_pipe_with_bounded_agent_startup,
    connect_pipe_with_typed_agent_startup, plan_bridge_startup,
};
