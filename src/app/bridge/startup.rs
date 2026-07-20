//! Native-host resident-agent startup planning.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BridgeStartupError {
    #[cfg(test)]
    OriginRejected,
    InvalidAttemptLimit,
    PipeUnavailable,
    PipeBusy,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BridgeStartupInputs {
    pub(crate) extension_origin_allowed: bool,
    pub(crate) agent_running: bool,
    pub(crate) max_pipe_connect_attempts: u8,
    pub(crate) pipe_ready_after_attempt: Option<u8>,
}

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BridgeStartupPlan {
    pub(crate) start_agent_once: bool,
    pub(crate) pipe_connect_attempts: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConnectedAgentPipe<T> {
    pub(crate) pipe: T,
    pub(crate) attempts: u8,
    pub(crate) started_agent: bool,
}

#[cfg(test)]
pub(crate) fn plan_bridge_startup(
    inputs: BridgeStartupInputs,
) -> Result<BridgeStartupPlan, BridgeStartupError> {
    if !inputs.extension_origin_allowed {
        return Err(BridgeStartupError::OriginRejected);
    }
    if inputs.max_pipe_connect_attempts == 0 {
        return Err(BridgeStartupError::InvalidAttemptLimit);
    }
    let start_agent_once = !inputs.agent_running;
    let pipe_connect_attempts = bounded_attempts(inputs)?;
    Ok(BridgeStartupPlan {
        start_agent_once,
        pipe_connect_attempts,
    })
}

#[cfg(test)]
fn bounded_attempts(inputs: BridgeStartupInputs) -> Result<u8, BridgeStartupError> {
    match inputs.pipe_ready_after_attempt {
        Some(ready_after) if ready_after > 0 && ready_after <= inputs.max_pipe_connect_attempts => {
            Ok(ready_after)
        }
        Some(_) | None => Err(BridgeStartupError::PipeUnavailable),
    }
}

#[cfg(test)]
pub(crate) fn connect_pipe_with_bounded_agent_startup<T>(
    max_pipe_connect_attempts: u8,
    mut connect_pipe: impl FnMut() -> Result<T, ()>,
    mut start_agent_once: impl FnMut() -> Result<(), ()>,
    mut wait_before_retry: impl FnMut(),
) -> Result<ConnectedAgentPipe<T>, BridgeStartupError> {
    if max_pipe_connect_attempts == 0 {
        return Err(BridgeStartupError::InvalidAttemptLimit);
    }
    let mut started_agent = false;
    for attempt in 1..=max_pipe_connect_attempts {
        if let Ok(pipe) = connect_pipe() {
            return Ok(ConnectedAgentPipe {
                pipe,
                attempts: attempt,
                started_agent,
            });
        }
        if !started_agent {
            start_agent_once().map_err(|()| BridgeStartupError::PipeUnavailable)?;
            started_agent = true;
        }
        if attempt < max_pipe_connect_attempts {
            wait_before_retry();
        }
    }
    Err(BridgeStartupError::PipeUnavailable)
}

#[cfg(any(windows, test))]
pub(crate) fn connect_pipe_with_typed_agent_startup<T, E>(
    max_pipe_connect_attempts: u8,
    mut connect_pipe: impl FnMut() -> Result<T, E>,
    mut start_agent_once: impl FnMut() -> Result<(), ()>,
    mut wait_before_retry: impl FnMut(),
    should_start_resident: impl Fn(&E) -> bool,
) -> Result<ConnectedAgentPipe<T>, BridgeStartupError> {
    if max_pipe_connect_attempts == 0 {
        return Err(BridgeStartupError::InvalidAttemptLimit);
    }
    let mut started_agent = false;
    let mut saw_non_startable_error = false;
    for attempt in 1..=max_pipe_connect_attempts {
        match connect_pipe() {
            Ok(pipe) => {
                return Ok(ConnectedAgentPipe {
                    pipe,
                    attempts: attempt,
                    started_agent,
                });
            }
            Err(error) if !started_agent && should_start_resident(&error) => {
                start_agent_once().map_err(|()| BridgeStartupError::PipeUnavailable)?;
                started_agent = true;
            }
            Err(_) => {
                saw_non_startable_error = true;
            }
        }
        if attempt < max_pipe_connect_attempts {
            wait_before_retry();
        }
    }
    if saw_non_startable_error {
        Err(BridgeStartupError::PipeBusy)
    } else {
        Err(BridgeStartupError::PipeUnavailable)
    }
}
