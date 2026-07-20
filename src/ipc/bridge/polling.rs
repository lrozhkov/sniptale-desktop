//! Polling resident-agent outbound forwarding.

use std::{
    io::{Read, Write},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use super::{
    AgentPipeReadShutdown, BridgeForwardError, BridgeForwardLogSink,
    forwarding::{forward_agent_response_batch, forward_browser_payloads_to_agent},
    logging::log_bridge_error,
    shutdown::is_expected_shutdown_forward_error,
};
use crate::ipc::NativeMessagingError;

const AGENT_RESPONSE_POLL_INTERVAL: Duration = Duration::from_millis(10);

pub(crate) trait AgentPipeResponseAvailability: Send {
    fn has_pending_agent_response(&self) -> std::io::Result<bool>;
}

pub(crate) fn forward_native_host_stream_polling_agent_with_shutdown<BR, BW, AR, AW, S>(
    browser_reader: &mut BR,
    browser_writer: &mut BW,
    mut agent_reader: AR,
    mut agent_writer: AW,
    shutdown: &S,
    log: Option<&BridgeForwardLogSink>,
) -> Result<(), BridgeForwardError>
where
    BR: Read + Send,
    BW: Write + Send,
    AR: Read + AgentPipeResponseAvailability + Send,
    AW: Write + Send,
    S: AgentPipeReadShutdown,
{
    thread::scope(|scope| {
        let browser_done = Arc::new(AtomicBool::new(false));
        let agent_log = log.cloned();
        let browser_done_ref = browser_done.clone();
        let agent_to_browser = scope.spawn(move || {
            loop {
                if agent_reader
                    .has_pending_agent_response()
                    .map_err(NativeMessagingError::Io)
                    .map_err(BridgeForwardError::AgentFrame)?
                {
                    if !forward_agent_response_batch(
                        &mut agent_reader,
                        browser_writer,
                        agent_log.as_ref(),
                    )? {
                        return Ok(());
                    }
                    continue;
                }
                if browser_done_ref.load(Ordering::Acquire) {
                    return Ok(());
                }
                thread::sleep(AGENT_RESPONSE_POLL_INTERVAL);
            }
        });

        let browser_log = log.cloned();
        let browser_result = forward_browser_payloads_to_agent(
            browser_reader,
            &mut agent_writer,
            browser_log.as_ref(),
        );
        browser_done.store(true, Ordering::Release);
        drop(agent_writer);
        shutdown.shutdown_agent_read();

        let agent_result = agent_to_browser
            .join()
            .map_err(|_| BridgeForwardError::ForwardThread)?;
        if let Err(error) = browser_result {
            log_bridge_error(log, "browser-to-agent-forward-error", &error);
            return Err(error);
        }
        if let Err(error) = agent_result
            && !is_expected_shutdown_forward_error(&error)
        {
            log_bridge_error(log, "agent-to-browser-forward-error", &error);
            return Err(error);
        }
        Ok(())
    })
}
