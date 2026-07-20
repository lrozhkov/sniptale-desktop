//! Browser/native-host duplex forwarding thread orchestration.

use std::{
    io::{Read, Write},
    thread,
    time::Duration,
};

use super::{
    AgentPipeReadShutdown, BridgeForwardError, BridgeForwardLogSink,
    forwarding::{forward_agent_response_batches, forward_browser_payloads_to_agent},
    logging::log_bridge_error,
    shutdown::is_expected_shutdown_forward_error,
};

// Windows process scheduling can delay a resident response after Native Messaging closes stdin.
// Keep this finite so a disconnected browser never leaves the pipe session alive indefinitely.
const BROWSER_EOF_RESPONSE_GRACE: Duration = Duration::from_millis(500);

#[cfg(test)]
pub(crate) fn forward_native_host_stream_duplex<BR, BW, AR, AW>(
    browser_reader: &mut BR,
    browser_writer: &mut BW,
    mut agent_reader: AR,
    mut agent_writer: AW,
) -> Result<(), BridgeForwardError>
where
    BR: Read + Send,
    BW: Write + Send,
    AR: Read + Send,
    AW: Write + Send,
{
    thread::scope(|scope| {
        let browser_to_agent = scope.spawn(move || {
            let result = forward_browser_payloads_to_agent(browser_reader, &mut agent_writer, None)
                .map(|_| ());
            drop(agent_writer);
            result
        });
        let agent_to_browser = scope
            .spawn(move || forward_agent_response_batches(&mut agent_reader, browser_writer, None));

        let browser_result = browser_to_agent
            .join()
            .map_err(|_| BridgeForwardError::ForwardThread)?;
        let agent_result = agent_to_browser
            .join()
            .map_err(|_| BridgeForwardError::ForwardThread)?;
        browser_result?;
        agent_result
    })
}

#[cfg(any(windows, test))]
pub(crate) fn forward_native_host_stream_duplex_with_shutdown<BR, BW, AR, AW, S>(
    browser_reader: &mut BR,
    browser_writer: &mut BW,
    mut agent_reader: AR,
    mut agent_writer: AW,
    shutdown: S,
    log: Option<&BridgeForwardLogSink>,
) -> Result<(), BridgeForwardError>
where
    BR: Read + Send,
    BW: Write + Send,
    AR: Read + Send,
    AW: Write + Send,
    S: AgentPipeReadShutdown,
{
    thread::scope(|scope| {
        let browser_log = log.cloned();
        let browser_to_agent = scope.spawn(move || {
            let result = forward_browser_payloads_to_agent(
                browser_reader,
                &mut agent_writer,
                browser_log.as_ref(),
            );
            drop(agent_writer);
            // Native Messaging probes may close stdin immediately after their final request.
            // Let the resident flush already-accepted responses before cancelling its blocking
            // read; the bounded grace preserves shutdown when the browser really disconnects.
            if matches!(&result, Ok(forwarded) if *forwarded > 0) {
                thread::sleep(BROWSER_EOF_RESPONSE_GRACE);
            }
            shutdown.shutdown_agent_read();
            result.map(|_| ())
        });
        let agent_log = log.cloned();
        let agent_to_browser = scope.spawn(move || {
            forward_agent_response_batches(&mut agent_reader, browser_writer, agent_log.as_ref())
        });

        let browser_result = browser_to_agent
            .join()
            .map_err(|_| BridgeForwardError::ForwardThread)?;
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
