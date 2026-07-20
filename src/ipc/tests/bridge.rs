use std::{
    error::Error,
    io::{Cursor, Read, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use crate::ipc::bridge::{
    AgentPipeResponseAvailability, forward_native_host_stream_duplex_with_shutdown,
    forward_native_host_stream_polling_agent_with_shutdown, forward_native_host_stream_with_log,
};

#[test]
fn duplex_grace_forwards_response_released_after_browser_eof() -> Result<(), Box<dyn Error>> {
    let gate = Arc::new(AtomicBool::new(false));
    let shutdown = SplitShutdown::default();
    let mut browser_reader = EofGateReader {
        incoming: Cursor::new(frame(extension_hello())?),
        gate: Arc::clone(&gate),
    };
    let mut browser_writer = Vec::new();
    let agent_reader = GateReader {
        incoming: Cursor::new(agent_response_frames(&[app_hello()])?),
        gate: Arc::clone(&gate),
        shutdown: shutdown.clone(),
        release_delay: Duration::from_millis(150),
        released: false,
    };
    let agent_writer = SharedWriter::default();

    forward_native_host_stream_duplex_with_shutdown(
        &mut browser_reader,
        &mut browser_writer,
        agent_reader,
        agent_writer,
        shutdown.clone(),
        None,
    )?;

    assert_eq!(read_one_payload(&browser_writer)?, app_hello());
    assert!(shutdown.was_called());
    Ok(())
}
use crate::ipc::{
    AgentPipeReadShutdown, AuthenticatedBridgeForwardError, AuthenticatedDuplexAgentPipe,
    BridgeForwardError, BridgeForwardLog, BridgeForwardLogSink, BridgeHello, PipeChallenge,
    PipeSecret, agent_pipe_response_header_payload, build_auth_response,
    forward_authenticated_native_host_stream,
    forward_authenticated_native_host_stream_duplex_with_authenticated_hook,
    forward_authenticated_native_host_stream_with_authenticated_hook,
    forward_authenticated_native_host_stream_with_log_and_authenticated_hook,
    forward_native_host_stream, forward_native_host_stream_duplex, read_payload, write_payload,
};

const FRAME_LIMIT: usize = 1024 * 1024;
const USER_SID: &str = "S-1-5-21-1000";
const INSTALL_ID: &str = "install-stable";
const CLIENT_NONCE: &str = "client-nonce-0001";
const SERVER_NONCE: &str = "server-nonce-0001";
const SESSION_ID: &str = "session-1";
const SECRET: &str = "per-install-secret";

#[test]
fn forwards_valid_browser_payload_to_agent_and_agent_response_to_browser()
-> Result<(), Box<dyn Error>> {
    let browser_input = frame(extension_hello())?;
    let agent_response = agent_response_frames(&[operation_failed()])?;
    let mut browser_reader = Cursor::new(browser_input.clone());
    let mut browser_writer = Vec::new();
    let mut agent_pipe = DuplexPipe::new(agent_response);

    forward_native_host_stream(&mut browser_reader, &mut browser_writer, &mut agent_pipe)?;

    assert_eq!(agent_pipe.written, browser_input);
    let forwarded = read_one_payload(&browser_writer)?;
    assert_eq!(forwarded, operation_failed());
    Ok(())
}

#[test]
fn forwards_multiple_agent_responses_to_browser_in_order() -> Result<(), Box<dyn Error>> {
    let browser_input = frame(extension_hello())?;
    let agent_response = agent_response_frames(&[command_accepted(), operation_failed()])?;
    let mut browser_reader = Cursor::new(browser_input);
    let mut browser_writer = Vec::new();
    let mut agent_pipe = DuplexPipe::new(agent_response);

    forward_native_host_stream(&mut browser_reader, &mut browser_writer, &mut agent_pipe)?;

    let forwarded = read_payloads(&browser_writer)?;
    assert_eq!(
        forwarded,
        vec![command_accepted().to_vec(), operation_failed().to_vec()]
    );
    Ok(())
}

#[test]
fn turn_based_forwarder_forwards_hello_then_controller_acquire_on_same_pipe()
-> Result<(), Box<dyn Error>> {
    let mut browser_input = frame(extension_hello())?;
    browser_input.extend(frame(extension_controller_acquire())?);
    let mut agent_response = agent_response_frames(&[app_hello()])?;
    agent_response.extend(agent_response_frames(&[app_controller_lease()])?);
    let mut browser_reader = Cursor::new(browser_input.clone());
    let mut browser_writer = Vec::new();
    let mut agent_pipe = DuplexPipe::new(agent_response);

    forward_native_host_stream(&mut browser_reader, &mut browser_writer, &mut agent_pipe)?;

    assert_eq!(agent_pipe.written, browser_input);
    assert_eq!(
        read_payloads(&browser_writer)?,
        vec![app_hello().to_vec(), app_controller_lease().to_vec()]
    );
    Ok(())
}

#[test]
fn polling_agent_forwarder_delivers_outbound_batch_without_next_browser_frame()
-> Result<(), Box<dyn Error>> {
    let browser_input = frame(extension_hello())?;
    let agent_response = {
        let mut frames = agent_response_frames(&[app_hello()])?;
        frames.extend(agent_response_frames(&[operation_failed()])?);
        frames
    };
    let mut browser_reader = Cursor::new(browser_input.clone());
    let mut browser_writer = Vec::new();
    let shutdown = SplitShutdown::default();
    let agent_reader = ShutdownAwareReader {
        incoming: Cursor::new(agent_response),
        shutdown: shutdown.clone(),
    };
    let agent_writer = SharedWriter::default();

    forward_native_host_stream_polling_agent_with_shutdown(
        &mut browser_reader,
        &mut browser_writer,
        agent_reader,
        agent_writer.clone(),
        &shutdown,
        None,
    )?;

    assert_eq!(agent_writer.bytes(), browser_input);
    assert_eq!(
        read_payloads(&browser_writer)?,
        vec![app_hello().to_vec(), operation_failed().to_vec()]
    );
    Ok(())
}

#[test]
fn logged_turn_based_forwarder_reports_malformed_post_hello_acquire_without_raw_payload()
-> Result<(), Box<dyn Error>> {
    let secret_profile_key = "profile-secret-value";
    let mut browser_input = frame(extension_hello())?;
    browser_input.extend(frame(
        &malformed_controller_acquire_missing_browser_family(secret_profile_key),
    )?);
    let agent_response = agent_response_frames(&[app_hello()])?;
    let mut browser_reader = Cursor::new(browser_input);
    let mut browser_writer = Vec::new();
    let mut agent_pipe = DuplexPipe::new(agent_response);
    let (log, entries) = CapturingBridgeLog::sink();

    let result = forward_native_host_stream_with_log(
        &mut browser_reader,
        &mut browser_writer,
        &mut agent_pipe,
        Some(&log),
    );

    assert!(matches!(
        result,
        Err(BridgeForwardError::BrowserProtocol(_))
    ));
    assert_eq!(agent_pipe.written, frame(extension_hello())?);
    assert_eq!(read_payloads(&browser_writer)?, vec![app_hello().to_vec()]);

    let entries = log_entries(&entries);
    assert!(
        entries.iter().any(|entry| {
            entry.phase == "browser-to-agent-forward-error"
                && entry.error_code.as_deref() == Some("browser-protocol")
        }),
        "expected browser protocol error log: {entries:?}"
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry.message.contains("type=extension.hello"))
    );
    let messages: Vec<_> = entries.iter().map(|entry| entry.message.as_str()).collect();
    assert!(
        messages
            .iter()
            .all(|message| !message.contains(secret_profile_key)),
        "log messages must not contain {secret_profile_key:?}: {messages:?}"
    );
    Ok(())
}

#[test]
fn duplex_forwarder_independently_forwards_browser_and_agent_streams() -> Result<(), Box<dyn Error>>
{
    let browser_input = Cursor::new(frame(extension_hello())?);
    let agent_input = Cursor::new(agent_response_frames(&[
        command_accepted(),
        operation_failed(),
    ])?);
    let browser_output = SharedWriter::default();
    let agent_output = SharedWriter::default();
    let mut browser_reader = browser_input;
    let mut browser_writer = browser_output.clone();

    forward_native_host_stream_duplex(
        &mut browser_reader,
        &mut browser_writer,
        agent_input,
        agent_output.clone(),
    )?;

    assert_eq!(agent_output.bytes(), frame(extension_hello())?);
    assert_eq!(
        read_payloads(&browser_output.bytes())?,
        vec![command_accepted().to_vec(), operation_failed().to_vec()]
    );
    Ok(())
}

#[test]
fn malformed_browser_payload_is_not_forwarded_to_agent() -> Result<(), Box<dyn Error>> {
    let mut browser_reader = Cursor::new(frame(br#"{"type":"extension.hello""#)?);
    let mut browser_writer = Vec::new();
    let mut agent_pipe = DuplexPipe::new(agent_response_frames(&[operation_failed()])?);

    let result =
        forward_native_host_stream(&mut browser_reader, &mut browser_writer, &mut agent_pipe);

    assert!(matches!(
        result,
        Err(BridgeForwardError::BrowserProtocol(_))
    ));
    assert!(agent_pipe.written.is_empty());
    assert!(browser_writer.is_empty());
    Ok(())
}

#[test]
fn logged_forwarder_does_not_log_unparsed_browser_type() -> Result<(), Box<dyn Error>> {
    let secret_type = "browser-secret-token";
    let payload = br#"{"type":"browser-secret-token","protocolVersion":1}"#;
    let mut browser_reader = Cursor::new(frame(payload)?);
    let mut browser_writer = Vec::new();
    let mut agent_pipe = DuplexPipe::new(Vec::new());
    let (log, entries) = CapturingBridgeLog::sink();

    let result = forward_native_host_stream_with_log(
        &mut browser_reader,
        &mut browser_writer,
        &mut agent_pipe,
        Some(&log),
    );

    assert!(matches!(
        result,
        Err(BridgeForwardError::BrowserProtocol(_))
    ));
    assert!(agent_pipe.written.is_empty());
    assert!(browser_writer.is_empty());
    assert_no_log_message_contains(&entries, secret_type);
    Ok(())
}

#[test]
fn malformed_agent_response_is_not_written_to_browser() -> Result<(), Box<dyn Error>> {
    let mut browser_reader = Cursor::new(frame(extension_hello())?);
    let mut browser_writer = Vec::new();
    let mut agent_response = frame(&agent_pipe_response_header_payload(1)?)?;
    agent_response.extend(frame(br#"{"type":"app.hello""#)?);
    let mut agent_pipe = DuplexPipe::new(agent_response);

    let result =
        forward_native_host_stream(&mut browser_reader, &mut browser_writer, &mut agent_pipe);

    assert!(matches!(result, Err(BridgeForwardError::AgentProtocol(_))));
    assert!(!agent_pipe.written.is_empty());
    assert!(browser_writer.is_empty());
    Ok(())
}

#[test]
fn logged_forwarder_does_not_log_unparsed_agent_type() -> Result<(), Box<dyn Error>> {
    let secret_type = "agent-secret-token";
    let mut browser_reader = Cursor::new(frame(extension_hello())?);
    let mut browser_writer = Vec::new();
    let mut agent_response = frame(&agent_pipe_response_header_payload(1)?)?;
    agent_response.extend(frame(
        br#"{"type":"agent-secret-token","protocolVersion":1}"#,
    )?);
    let mut agent_pipe = DuplexPipe::new(agent_response);
    let (log, entries) = CapturingBridgeLog::sink();

    let result = forward_native_host_stream_with_log(
        &mut browser_reader,
        &mut browser_writer,
        &mut agent_pipe,
        Some(&log),
    );

    assert!(matches!(result, Err(BridgeForwardError::AgentProtocol(_))));
    assert!(!agent_pipe.written.is_empty());
    assert!(browser_writer.is_empty());
    assert!(
        log_messages(&entries)
            .iter()
            .any(|message| message.contains("type=extension.hello"))
    );
    assert_no_log_message_contains(&entries, secret_type);
    Ok(())
}

#[test]
fn clean_browser_eof_does_not_touch_agent_pipe() -> Result<(), Box<dyn Error>> {
    let mut browser_reader = Cursor::new(Vec::<u8>::new());
    let mut browser_writer = Vec::new();
    let mut agent_pipe = DuplexPipe::new(agent_response_frames(&[operation_failed()])?);

    forward_native_host_stream(&mut browser_reader, &mut browser_writer, &mut agent_pipe)?;

    assert!(agent_pipe.written.is_empty());
    assert!(browser_writer.is_empty());
    Ok(())
}

#[test]
fn authenticated_forwarder_completes_pipe_handshake_before_forwarding() -> Result<(), Box<dyn Error>>
{
    let browser_input = frame(extension_hello())?;
    let mut agent_input = frame_json(&challenge())?;
    agent_input.extend(agent_response_frames(&[operation_failed()])?);
    let mut browser_reader = Cursor::new(browser_input.clone());
    let mut browser_writer = Vec::new();
    let mut agent_pipe = DuplexPipe::new(agent_input);

    forward_authenticated_native_host_stream(
        &mut browser_reader,
        &mut browser_writer,
        &mut agent_pipe,
        &hello(),
        &secret()?,
    )?;

    let mut written = Cursor::new(agent_pipe.written);
    let hello_payload = read_payload(&mut written, FRAME_LIMIT)?.ok_or("missing bridge hello")?;
    let response_payload =
        read_payload(&mut written, FRAME_LIMIT)?.ok_or("missing bridge auth response")?;
    let forwarded_payload =
        read_payload(&mut written, FRAME_LIMIT)?.ok_or("missing forwarded browser payload")?;

    assert_eq!(
        serde_json::from_slice::<BridgeHello>(&hello_payload)?,
        hello()
    );
    assert_eq!(
        response_payload,
        serde_json::to_vec(&build_auth_response(&challenge(), &secret()?)?)?
    );
    assert_eq!(forwarded_payload, extension_hello());
    assert_eq!(read_one_payload(&browser_writer)?, operation_failed());
    Ok(())
}

#[test]
fn authenticated_forwarder_rejects_malformed_challenge_before_forwarding()
-> Result<(), Box<dyn Error>> {
    let browser_input = frame(extension_hello())?;
    let mut browser_reader = Cursor::new(browser_input);
    let mut browser_writer = Vec::new();
    let mut agent_pipe = DuplexPipe::new(frame(br#"{"sessionId":"unterminated""#)?);

    let result = forward_authenticated_native_host_stream(
        &mut browser_reader,
        &mut browser_writer,
        &mut agent_pipe,
        &hello(),
        &secret()?,
    );

    assert!(matches!(
        result,
        Err(AuthenticatedBridgeForwardError::Handshake(_))
    ));
    let mut written = Cursor::new(agent_pipe.written);
    assert!(read_payload(&mut written, FRAME_LIMIT)?.is_some());
    assert!(read_payload(&mut written, FRAME_LIMIT)?.is_none());
    assert!(browser_writer.is_empty());
    Ok(())
}

#[test]
fn authenticated_forwarder_runs_hook_after_successful_auth() -> Result<(), Box<dyn Error>> {
    let browser_input = frame(extension_hello())?;
    let mut agent_input = frame_json(&challenge())?;
    agent_input.extend(agent_response_frames(&[operation_failed()])?);
    let mut browser_reader = Cursor::new(browser_input);
    let mut browser_writer = Vec::new();
    let mut agent_pipe = DuplexPipe::new(agent_input);
    let mut hook_calls = 0;

    forward_authenticated_native_host_stream_with_authenticated_hook(
        &mut browser_reader,
        &mut browser_writer,
        &mut agent_pipe,
        &hello(),
        &secret()?,
        || hook_calls += 1,
    )?;

    assert_eq!(hook_calls, 1);
    assert_eq!(read_one_payload(&browser_writer)?, operation_failed());
    Ok(())
}

#[test]
fn authenticated_logged_forwarder_uses_turn_based_control_order() -> Result<(), Box<dyn Error>> {
    let mut browser_input = frame(extension_hello())?;
    browser_input.extend(frame(extension_controller_acquire())?);
    let mut agent_input = frame_json(&challenge())?;
    agent_input.extend(agent_response_frames(&[app_hello()])?);
    agent_input.extend(agent_response_frames(&[app_controller_lease()])?);
    let mut browser_reader = Cursor::new(browser_input.clone());
    let mut browser_writer = Vec::new();
    let mut agent_pipe = DuplexPipe::new(agent_input);
    let mut hook_calls = 0;

    forward_authenticated_native_host_stream_with_log_and_authenticated_hook(
        &mut browser_reader,
        &mut browser_writer,
        &mut agent_pipe,
        &hello(),
        &secret()?,
        None,
        || hook_calls += 1,
    )?;

    let mut written = Cursor::new(agent_pipe.written);
    let hello_payload = read_payload(&mut written, FRAME_LIMIT)?.ok_or("missing bridge hello")?;
    let response_payload =
        read_payload(&mut written, FRAME_LIMIT)?.ok_or("missing bridge auth response")?;
    let first_forwarded =
        read_payload(&mut written, FRAME_LIMIT)?.ok_or("missing extension hello")?;
    let second_forwarded =
        read_payload(&mut written, FRAME_LIMIT)?.ok_or("missing controller acquire")?;
    assert_eq!(
        serde_json::from_slice::<BridgeHello>(&hello_payload)?,
        hello()
    );
    assert_eq!(
        response_payload,
        serde_json::to_vec(&build_auth_response(&challenge(), &secret()?)?)?
    );
    assert_eq!(first_forwarded, extension_hello());
    assert_eq!(second_forwarded, extension_controller_acquire());
    assert_eq!(hook_calls, 1);
    assert_eq!(
        read_payloads(&browser_writer)?,
        vec![app_hello().to_vec(), app_controller_lease().to_vec()]
    );
    Ok(())
}

#[test]
fn authenticated_duplex_forwarder_delivers_agent_response_without_browser_request()
-> Result<(), Box<dyn Error>> {
    let mut agent_input = frame_json(&challenge())?;
    agent_input.extend(agent_response_frames(&[operation_failed()])?);
    let mut browser_reader = Cursor::new(Vec::new());
    let mut browser_writer = Vec::new();
    let agent_written = SharedWriter::default();
    let shutdown = SplitShutdown::default();
    let agent_pipe = SplitAgentPipe::new(agent_input, agent_written.clone(), shutdown.clone());
    let mut hook_calls = 0;

    forward_authenticated_native_host_stream_duplex_with_authenticated_hook(
        &mut browser_reader,
        &mut browser_writer,
        agent_pipe,
        &hello(),
        &secret()?,
        None,
        || hook_calls += 1,
    )?;

    let mut written = Cursor::new(agent_written.bytes());
    let hello_payload = read_payload(&mut written, FRAME_LIMIT)?.ok_or("missing bridge hello")?;
    let response_payload =
        read_payload(&mut written, FRAME_LIMIT)?.ok_or("missing bridge auth response")?;
    assert_eq!(hook_calls, 1);
    assert_eq!(
        serde_json::from_slice::<BridgeHello>(&hello_payload)?,
        hello()
    );
    assert_eq!(
        response_payload,
        serde_json::to_vec(&build_auth_response(&challenge(), &secret()?)?)?
    );
    assert!(read_payload(&mut written, FRAME_LIMIT)?.is_none());
    assert_eq!(read_one_payload(&browser_writer)?, operation_failed());
    assert!(shutdown.was_called());
    Ok(())
}

#[test]
fn authenticated_forwarder_does_not_run_hook_after_failed_auth() -> Result<(), Box<dyn Error>> {
    let browser_input = frame(extension_hello())?;
    let mut browser_reader = Cursor::new(browser_input);
    let mut browser_writer = Vec::new();
    let mut agent_pipe = DuplexPipe::new(frame(br#"{"sessionId":"unterminated""#)?);
    let mut hook_calls = 0;

    let result = forward_authenticated_native_host_stream_with_authenticated_hook(
        &mut browser_reader,
        &mut browser_writer,
        &mut agent_pipe,
        &hello(),
        &secret()?,
        || hook_calls += 1,
    );

    assert!(matches!(
        result,
        Err(AuthenticatedBridgeForwardError::Handshake(_))
    ));
    assert_eq!(hook_calls, 0);
    assert!(browser_writer.is_empty());
    Ok(())
}

fn frame(payload: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut output = Vec::new();
    write_payload(&mut output, payload, FRAME_LIMIT)?;
    Ok(output)
}

fn frame_json(value: &impl serde::Serialize) -> Result<Vec<u8>, Box<dyn Error>> {
    frame(&serde_json::to_vec(value)?)
}

fn agent_response_frames(responses: &[&[u8]]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut output = frame(&agent_pipe_response_header_payload(responses.len())?)?;
    for response in responses {
        output.extend(frame(response)?);
    }
    Ok(output)
}

fn read_one_payload(bytes: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut cursor = Cursor::new(bytes.to_vec());
    read_payload(&mut cursor, FRAME_LIMIT)?.ok_or_else(|| "missing frame".into())
}

fn read_payloads(bytes: &[u8]) -> Result<Vec<Vec<u8>>, Box<dyn Error>> {
    let mut cursor = Cursor::new(bytes.to_vec());
    let mut payloads = Vec::new();
    while let Some(payload) = read_payload(&mut cursor, FRAME_LIMIT)? {
        payloads.push(payload);
    }
    Ok(payloads)
}

fn hello() -> BridgeHello {
    BridgeHello {
        current_user_sid: USER_SID.to_owned(),
        install_id: INSTALL_ID.to_owned(),
        extension_origin: "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/".to_owned(),
        client_nonce: CLIENT_NONCE.to_owned(),
    }
}

fn challenge() -> PipeChallenge {
    PipeChallenge {
        algorithm: "pipe-auth-v2-hmac-sha256".to_owned(),
        session_id: SESSION_ID.to_owned(),
        install_id: INSTALL_ID.to_owned(),
        client_nonce: CLIENT_NONCE.to_owned(),
        server_nonce: SERVER_NONCE.to_owned(),
        expires_at_ms: 6_000,
    }
}

fn secret() -> Result<PipeSecret, Box<dyn Error>> {
    Ok(PipeSecret::new(SECRET)?)
}

fn extension_hello() -> &'static [u8] {
    br#"{
        "type": "extension.hello",
        "protocolVersion": 1,
        "settingsSchemaVersion": 1,
        "extensionVersion": "0.1.0",
        "extensionId": "abcdefghijklmnopqrstuvwxzyabcdef",
        "minAppVersion": "0.1.0",
        "supportedProtocolVersions": [1],
        "supportedSettingsSchemaVersions": [1]
    }"#
}

fn extension_controller_acquire() -> &'static [u8] {
    br#"{
        "type": "extension.controller.acquire",
        "protocolVersion": 1,
        "connectionId": "connection-1",
        "browserFamily": "chrome",
        "profileKey": "profile-1",
        "documentId": "document-1",
        "requestedAtEpochMs": 1,
        "reason": "initial-connect"
    }"#
}

fn malformed_controller_acquire_missing_browser_family(profile_key: &str) -> Vec<u8> {
    format!(
        r#"{{
        "type": "extension.controller.acquire",
        "protocolVersion": 1,
        "connectionId": "connection-1",
        "profileKey": "{profile_key}",
        "documentId": "document-1",
        "requestedAtEpochMs": 1,
        "reason": "initial-connect"
    }}"#
    )
    .into_bytes()
}

fn app_hello() -> &'static [u8] {
    br#"{
        "type": "app.hello",
        "protocolVersion": 1,
        "settingsSchemaVersion": 1,
        "supportedProtocolVersions": [1],
        "supportedSettingsSchemaVersions": [1],
        "minExtensionVersion": "0.1.0",
        "appInstanceId": "sniptale-native-agent",
        "platform": {
            "kind": "windows",
            "version": "Windows_NT",
            "arch": "x64"
        },
        "install": {
            "appVersion": "0.1.0",
            "installerVersion": "0.1.0",
            "nativeHostManifestVersion": "0.1.0",
            "appCacheSchemaVersion": 1,
            "updateChannel": "stable",
            "platform": {
                "kind": "windows",
                "version": "Windows_NT",
                "arch": "x64"
            },
            "autostart": {
                "supported": true,
                "enabled": false,
                "method": "windows-hkcu-run"
            },
            "signedBinary": false,
            "rollbackProtected": true,
            "packageIntegrity": "unknown"
        },
        "capabilities": {
            "capture": {
                "screenshotModes": ["screen"],
                "videoModes": ["screen"],
                "supportsFreezeRegionSelection": false
            },
            "audio": {
                "supportsSystemAudio": false,
                "supportsMicrophone": false,
                "supportsMixedAudio": false,
                "microphoneDevices": [],
                "unavailableReasons": ["unknown"]
            },
            "codecs": {
                "containers": ["mp4"],
                "video": ["h264"],
                "audio": ["aac"],
                "hardwareEncoderAvailable": false,
                "unavailableReasons": []
            },
            "limits": {
                "maxWidth": 7680,
                "maxHeight": 4320,
                "maxFps": 60,
                "maxChunkBytes": 524288,
                "maxRecordingBytes": 8589934592,
                "maxScreenshotBytes": 268435456
            }
        }
    }"#
}

fn app_controller_lease() -> &'static [u8] {
    br#"{
        "type": "app.controller.lease",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "controller": {
            "extensionId": "abcdefghijklmnopqrstuvwxzyabcdef",
            "browserFamily": "chrome",
            "profileKey": "profile-1",
            "documentId": "document-1",
            "connectionId": "connection-1"
        },
        "status": "granted",
        "expiresAtEpochMs": 60001
    }"#
}

fn operation_failed() -> &'static [u8] {
    br#"{
        "type": "app.operation.failed",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "operation": "screenshot",
        "operationId": "command-1",
        "phase": "capture",
        "error": {
            "code": "unsupported-capability",
            "message": "native capture pipeline is unavailable",
            "recoverable": false
        },
        "occurredAtEpochMs": 1
    }"#
}

fn command_accepted() -> &'static [u8] {
    br#"{
        "type": "app.command.accepted",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "commandId": "command-1",
        "operation": "screenshot",
        "acceptedAtEpochMs": 1
    }"#
}

struct DuplexPipe {
    incoming: Cursor<Vec<u8>>,
    written: Vec<u8>,
}

struct SplitAgentPipe {
    incoming: Cursor<Vec<u8>>,
    written: SharedWriter,
    shutdown: SplitShutdown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BridgeLogEvent {
    phase: &'static str,
    message: String,
    error_code: Option<String>,
}

struct CapturingBridgeLog {
    entries: Arc<Mutex<Vec<BridgeLogEvent>>>,
}

#[derive(Clone, Default)]
struct SplitShutdown {
    called: Arc<AtomicBool>,
}

#[derive(Clone, Default)]
struct SharedWriter {
    bytes: Arc<Mutex<Vec<u8>>>,
}

impl SharedWriter {
    fn bytes(&self) -> Vec<u8> {
        self.bytes
            .lock()
            .map(|bytes| bytes.clone())
            .unwrap_or_default()
    }
}

impl CapturingBridgeLog {
    fn sink() -> (BridgeForwardLogSink, Arc<Mutex<Vec<BridgeLogEvent>>>) {
        let entries = Arc::new(Mutex::new(Vec::new()));
        (
            Arc::new(Self {
                entries: entries.clone(),
            }),
            entries,
        )
    }
}

impl BridgeForwardLog for CapturingBridgeLog {
    fn event(&self, phase: &'static str, message: &str, error_code: Option<&str>) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.push(BridgeLogEvent {
                phase,
                message: message.to_owned(),
                error_code: error_code.map(str::to_owned),
            });
        }
    }
}

fn log_messages(entries: &Arc<Mutex<Vec<BridgeLogEvent>>>) -> Vec<String> {
    log_entries(entries)
        .into_iter()
        .map(|entry| entry.message)
        .collect()
}

fn log_entries(entries: &Arc<Mutex<Vec<BridgeLogEvent>>>) -> Vec<BridgeLogEvent> {
    entries
        .lock()
        .map_or_else(|_| Vec::new(), |entries| entries.clone())
}

fn assert_no_log_message_contains(entries: &Arc<Mutex<Vec<BridgeLogEvent>>>, needle: &str) {
    let messages = log_messages(entries);
    assert!(
        messages.iter().all(|message| !message.contains(needle)),
        "log messages must not contain {needle:?}: {messages:?}"
    );
}

impl Write for SharedWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        let mut bytes = self
            .bytes
            .lock()
            .map_err(|_| std::io::Error::other("shared writer poisoned"))?;
        bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl DuplexPipe {
    fn new(incoming: Vec<u8>) -> Self {
        Self {
            incoming: Cursor::new(incoming),
            written: Vec::new(),
        }
    }
}

impl SplitAgentPipe {
    fn new(incoming: Vec<u8>, written: SharedWriter, shutdown: SplitShutdown) -> Self {
        Self {
            incoming: Cursor::new(incoming),
            written,
            shutdown,
        }
    }
}

impl Read for DuplexPipe {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.incoming.read(buffer)
    }
}

impl Write for DuplexPipe {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.written.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Read for SplitAgentPipe {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.incoming.read(buffer)
    }
}

impl Write for SplitAgentPipe {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.written.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.written.flush()
    }
}

impl AuthenticatedDuplexAgentPipe for SplitAgentPipe {
    type Reader = ShutdownAwareReader;
    type Writer = SharedWriter;
    type Shutdown = SplitShutdown;

    fn split_for_duplex(self) -> std::io::Result<(Self::Reader, Self::Writer, Self::Shutdown)> {
        Ok((
            ShutdownAwareReader {
                incoming: self.incoming,
                shutdown: self.shutdown.clone(),
            },
            self.written,
            self.shutdown,
        ))
    }
}

struct ShutdownAwareReader {
    incoming: Cursor<Vec<u8>>,
    shutdown: SplitShutdown,
}

impl AgentPipeResponseAvailability for ShutdownAwareReader {
    fn has_pending_agent_response(&self) -> std::io::Result<bool> {
        let position = usize::try_from(self.incoming.position()).unwrap_or(usize::MAX);
        Ok(position < self.incoming.get_ref().len())
    }
}

impl Read for ShutdownAwareReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.incoming.read(buffer)?;
        if read > 0 {
            return Ok(read);
        }
        for _ in 0..20 {
            if self.shutdown.was_called() {
                return Ok(0);
            }
            thread::sleep(Duration::from_millis(5));
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "agent reader was not shut down",
        ))
    }
}

impl SplitShutdown {
    fn was_called(&self) -> bool {
        self.called.load(Ordering::Acquire)
    }
}

impl AgentPipeReadShutdown for SplitShutdown {
    fn shutdown_agent_read(&self) {
        self.called.store(true, Ordering::Release);
    }
}

struct EofGateReader {
    incoming: Cursor<Vec<u8>>,
    gate: Arc<AtomicBool>,
}
impl Read for EofGateReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.incoming.read(buffer)?;
        if read == 0 {
            self.gate.store(true, Ordering::Release);
        }
        Ok(read)
    }
}

struct GateReader {
    incoming: Cursor<Vec<u8>>,
    gate: Arc<AtomicBool>,
    shutdown: SplitShutdown,
    release_delay: Duration,
    released: bool,
}
impl Read for GateReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        while !self.gate.load(Ordering::Acquire) && !self.shutdown.was_called() {
            thread::sleep(Duration::from_millis(1));
        }
        if self.shutdown.was_called() {
            return Ok(0);
        }
        if !self.released {
            thread::sleep(self.release_delay);
            self.released = true;
            if self.shutdown.was_called() {
                return Ok(0);
            }
        }
        self.incoming.read(buffer)
    }
}
