//! Native-host browser/agent frame forwarding.

use std::io::{Read, Write};

use super::{
    AGENT_PIPE_RESPONSE_HEADER_MAX_BYTES, BridgeForwardError, BridgeForwardLogSink,
    parse_agent_pipe_response_header,
};
use crate::{
    ipc::{read_payload, write_payload},
    protocol::{
        EXTENSION_TO_HOST_MAX_MESSAGE_BYTES, HOST_TO_EXTENSION_MAX_MESSAGE_BYTES,
        parse_wire_message, parse_wire_message_with_limit,
    },
};

use super::logging::{log_bridge_event, wire_message_type};

#[cfg(test)]
pub(super) fn forward_browser_payload(
    payload: &[u8],
    browser_writer: &mut impl Write,
    agent_pipe: &mut (impl Read + Write),
    log: Option<&BridgeForwardLogSink>,
) -> Result<(), BridgeForwardError> {
    let message = parse_wire_message(payload).map_err(BridgeForwardError::BrowserProtocol)?;
    let message_type = wire_message_type(&message);
    let message = format!("native-host bridge received browser message type={message_type}");
    log_bridge_event(log, "browser-frame-received", &message);
    write_payload(agent_pipe, payload, EXTENSION_TO_HOST_MAX_MESSAGE_BYTES)
        .map_err(BridgeForwardError::AgentFrame)?;
    let message = format!("native-host bridge forwarded agent message type={message_type}");
    log_bridge_event(log, "agent-frame-forwarded", &message);

    forward_agent_response_batch(agent_pipe, browser_writer, log).map(|_| ())
}

#[cfg(any(windows, test))]
pub(super) fn forward_browser_payloads_to_agent(
    browser_reader: &mut impl Read,
    agent_writer: &mut impl Write,
    log: Option<&BridgeForwardLogSink>,
) -> Result<usize, BridgeForwardError> {
    let mut forwarded: usize = 0;
    while let Some(payload) = read_payload(browser_reader, EXTENSION_TO_HOST_MAX_MESSAGE_BYTES)
        .map_err(BridgeForwardError::BrowserFrame)?
    {
        let message = parse_wire_message(&payload).map_err(BridgeForwardError::BrowserProtocol)?;
        let message_type = wire_message_type(&message);
        let message = format!("native-host bridge received browser message type={message_type}");
        log_bridge_event(log, "browser-frame-received", &message);
        write_payload(agent_writer, &payload, EXTENSION_TO_HOST_MAX_MESSAGE_BYTES)
            .map_err(BridgeForwardError::AgentFrame)?;
        forwarded = forwarded.saturating_add(1);
        let message = format!("native-host bridge forwarded agent message type={message_type}");
        log_bridge_event(log, "agent-frame-forwarded", &message);
    }
    Ok(forwarded)
}

#[cfg(any(windows, test))]
pub(super) fn forward_agent_response_batches(
    agent_reader: &mut impl Read,
    browser_writer: &mut impl Write,
    log: Option<&BridgeForwardLogSink>,
) -> Result<(), BridgeForwardError> {
    while forward_agent_response_batch(agent_reader, browser_writer, log)? {}
    Ok(())
}

pub(super) fn forward_agent_response_batch(
    agent_reader: &mut impl Read,
    browser_writer: &mut impl Write,
    log: Option<&BridgeForwardLogSink>,
) -> Result<bool, BridgeForwardError> {
    let Some(header_payload) = read_payload(agent_reader, AGENT_PIPE_RESPONSE_HEADER_MAX_BYTES)
        .map_err(BridgeForwardError::AgentFrame)?
    else {
        return Ok(false);
    };
    let response_count = parse_agent_pipe_response_header(&header_payload)
        .map_err(BridgeForwardError::AgentResponseHeader)?;
    let message =
        format!("native-host bridge read agent response header responseCount={response_count}");
    log_bridge_event(log, "agent-response-header-read", &message);
    for _ in 0..response_count {
        let response_payload = read_payload(agent_reader, HOST_TO_EXTENSION_MAX_MESSAGE_BYTES)
            .map_err(BridgeForwardError::AgentFrame)?
            .ok_or(BridgeForwardError::AgentResponseMissing)?;
        let message =
            parse_wire_message_with_limit(&response_payload, HOST_TO_EXTENSION_MAX_MESSAGE_BYTES)
                .map_err(BridgeForwardError::AgentProtocol)?;
        let message_type = wire_message_type(&message);
        let message = format!("native-host bridge writing browser message type={message_type}");
        log_bridge_event(log, "browser-frame-write-start", &message);
        write_payload(
            browser_writer,
            &response_payload,
            HOST_TO_EXTENSION_MAX_MESSAGE_BYTES,
        )
        .map_err(BridgeForwardError::BrowserFrame)?;
        let message = format!("native-host bridge wrote browser message type={message_type}");
        log_bridge_event(log, "browser-frame-written", &message);
    }
    Ok(true)
}
