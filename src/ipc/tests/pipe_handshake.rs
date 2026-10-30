use std::{
    error::Error,
    io::{Cursor, Read, Write},
};

use crate::ipc::{
    BridgeAuthResponse, BridgeHello, NativeMessagingError, PipeAuthError, PipeChallenge,
    PipeHandshakeError, PipeSecret, PipeServerAuth, authenticate_client_pipe,
    authenticate_server_pipe, build_auth_response, read_payload, write_payload,
};

const USER_SID: &str = "S-1-5-21-1000";
const INSTALL_ID: &str = "install-stable";
const SECRET: &str = "per-install-secret";
const ORIGIN: &str = "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/";
const CLIENT_NONCE: &str = "client-nonce-0001";
const SERVER_NONCE: &str = "server-nonce-0001";
const SESSION_ID: &str = "session-1";
const AUTH_FRAME_LIMIT: usize = 16 * 1024;

#[test]
fn client_handshake_writes_hello_then_auth_response() -> Result<(), Box<dyn Error>> {
    let challenge = challenge();
    let mut pipe = DuplexPipe::new(frame_json(&challenge)?);

    authenticate_client_pipe(&mut pipe, &hello(), &secret()?)?;

    let mut written = Cursor::new(pipe.written);
    let hello_payload = read_payload(&mut written, AUTH_FRAME_LIMIT)?.ok_or("missing hello")?;
    let response_payload =
        read_payload(&mut written, AUTH_FRAME_LIMIT)?.ok_or("missing response")?;
    let observed_hello: BridgeHello = serde_json::from_slice(&hello_payload)?;
    let observed_response: BridgeAuthResponse = serde_json::from_slice(&response_payload)?;
    assert_eq!(observed_hello, hello());
    assert_eq!(
        observed_response,
        build_auth_response(&challenge, &secret()?)?
    );
    Ok(())
}

#[test]
fn server_handshake_authenticates_current_user_session() -> Result<(), Box<dyn Error>> {
    let challenge = challenge();
    let response = build_auth_response(&challenge, &secret()?)?;
    let mut input = frame_json(&hello())?;
    input.extend(frame_json(&response)?);
    let mut pipe = DuplexPipe::new(input);
    let mut auth = server(2, 2)?;

    let session =
        authenticate_server_pipe(&mut pipe, &mut auth, 1_000, 5_000, SESSION_ID, SERVER_NONCE)?;

    assert_eq!(session.session_id, SESSION_ID);
    assert_eq!(session.current_user_sid, USER_SID);
    let mut written = Cursor::new(pipe.written);
    let challenge_payload =
        read_payload(&mut written, AUTH_FRAME_LIMIT)?.ok_or("missing challenge")?;
    let observed_challenge: PipeChallenge = serde_json::from_slice(&challenge_payload)?;
    assert_eq!(observed_challenge, challenge);
    Ok(())
}

#[test]
fn malformed_challenge_fails_client_before_response_write() -> Result<(), Box<dyn Error>> {
    let mut pipe = DuplexPipe::new(frame_raw(br#"{"sessionId":"unterminated""#)?);

    let result = authenticate_client_pipe(&mut pipe, &hello(), &secret()?);

    assert!(matches!(result, Err(PipeHandshakeError::Json(_))));
    let mut written = Cursor::new(pipe.written);
    assert!(read_payload(&mut written, AUTH_FRAME_LIMIT)?.is_some());
    assert!(read_payload(&mut written, AUTH_FRAME_LIMIT)?.is_none());
    Ok(())
}

#[test]
fn oversized_hello_is_rejected_before_auth_state_mutates() -> Result<(), Box<dyn Error>> {
    let mut input = Vec::new();
    let declared = u32::try_from(AUTH_FRAME_LIMIT + 1)?;
    input.extend_from_slice(&declared.to_le_bytes());
    let mut pipe = DuplexPipe::new(input);
    let mut auth = server(1, 1)?;

    let result =
        authenticate_server_pipe(&mut pipe, &mut auth, 1_000, 5_000, SESSION_ID, SERVER_NONCE);

    assert!(matches!(
        result,
        Err(PipeHandshakeError::Frame(
            NativeMessagingError::MessageTooLarge { .. }
        ))
    ));
    assert_eq!(pipe.written.len(), 0);
    Ok(())
}

#[test]
fn malformed_response_session_id_releases_serial_handshake_capacity() -> Result<(), Box<dyn Error>>
{
    let challenge = challenge();
    let mut response = build_auth_response(&challenge, &secret()?)?;
    response.session_id = "session-tampered".to_owned();
    let mut input = frame_json(&hello())?;
    input.extend(frame_json(&response)?);
    let mut pipe = DuplexPipe::new(input);
    let mut auth = server(1, 1)?;

    let result =
        authenticate_server_pipe(&mut pipe, &mut auth, 1_000, 5_000, SESSION_ID, SERVER_NONCE);

    assert!(matches!(
        result,
        Err(PipeHandshakeError::Auth(PipeAuthError::InvalidResponse))
    ));
    assert!(
        auth.issue_challenge(hello(), 2_000, 5_000, "session-2", "server-nonce-0002")
            .is_ok()
    );
    Ok(())
}

fn secret() -> Result<PipeSecret, PipeAuthError> {
    PipeSecret::new(SECRET)
}

fn hello() -> BridgeHello {
    BridgeHello {
        current_user_sid: USER_SID.to_owned(),
        install_id: INSTALL_ID.to_owned(),
        extension_origin: ORIGIN.to_owned(),
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

fn server(max_sessions: usize, queue_capacity: usize) -> Result<PipeServerAuth, PipeAuthError> {
    PipeServerAuth::new(
        USER_SID,
        INSTALL_ID,
        &[ORIGIN],
        Some(secret()?),
        max_sessions,
        queue_capacity,
    )
}

fn frame_json(value: &impl serde::Serialize) -> Result<Vec<u8>, Box<dyn Error>> {
    frame_raw(&serde_json::to_vec(value)?)
}

fn frame_raw(payload: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut output = Vec::new();
    write_payload(&mut output, payload, AUTH_FRAME_LIMIT)?;
    Ok(output)
}

struct DuplexPipe {
    incoming: Cursor<Vec<u8>>,
    written: Vec<u8>,
}

impl DuplexPipe {
    fn new(incoming: Vec<u8>) -> Self {
        Self {
            incoming: Cursor::new(incoming),
            written: Vec::new(),
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
