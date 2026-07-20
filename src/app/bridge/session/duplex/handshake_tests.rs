use std::{
    io::{Cursor, Read, Write},
    sync::{Arc, Mutex},
};

use crate::{
    app::bridge::session::AgentPipeConnectionContext,
    app::bridge::session::{
        AgentPipeConnectionConsent, AgentPipeConsentDecision, AgentPipeConsentRequest,
        AgentPipeSessionConfig, AgentPipeSessionError,
    },
    ipc::{
        BridgeAuthResponse, BridgeHello, PipeAuthError, PipeChallenge, PipeSecret, PipeServerAuth,
        build_auth_response, write_payload,
    },
};

use super::{
    AgentPipeConsentRuntime, authenticate_shared_server_pipe,
    authenticate_shared_server_pipe_with_consent, handshake_timeout,
};

#[test]
fn malformed_response_session_id_releases_pending_capacity()
-> Result<(), Box<dyn std::error::Error>> {
    let auth = Arc::new(Mutex::new(auth(1)?));
    let config = config("session-1", "server-nonce-0001");
    let response = tampered_session_response("session-1", "session-tampered")?;
    let mut pipe = PipeIo::new(frames(&hello(), &response)?);

    assert!(
        authenticate_shared_server_pipe(&mut pipe, &auth, &config).is_err(),
        "tampered response session must fail auth"
    );

    assert!(
        auth.lock()
            .map_err(|_| PipeAuthError::InvalidResponse)?
            .issue_challenge(hello(), 2_000, 5_000, "session-2", "server-nonce-0002")
            .is_ok()
    );
    Ok(())
}

#[test]
fn consent_allow_keeps_authenticated_session() -> Result<(), Box<dyn std::error::Error>> {
    let auth = Arc::new(Mutex::new(auth(1)?));
    let config = config("session-1", "server-nonce-0001");
    let response = valid_response()?;
    let mut pipe = PipeIo::new(frames(&hello(), &response)?);
    let consent = MockConsent::new(AgentPipeConsentDecision::Allowed);
    let context = context();

    let session = authenticate_shared_server_pipe_with_consent(
        &mut pipe,
        &auth,
        &config,
        AgentPipeConsentRuntime {
            consent: Some(&consent),
            context: Some(&context),
        },
        None,
    )?;

    assert_eq!(session.extension_origin, ORIGIN);
    assert_eq!(consent.calls(), 1);
    Ok(())
}

#[test]
fn consent_warning_keeps_authenticated_session() -> Result<(), Box<dyn std::error::Error>> {
    let auth = Arc::new(Mutex::new(auth(1)?));
    let config = config("session-1", "server-nonce-0001");
    let response = valid_response()?;
    let mut pipe = PipeIo::new(frames(&hello(), &response)?);
    let consent = MockConsent::new(AgentPipeConsentDecision::AllowedWithWarning {
        code: "resident_pipe.connection_consent_store_write_failed",
        message: "resident pipe accepted native-host session, but trusted connection was not persisted",
    });
    let context = context();

    let session = authenticate_shared_server_pipe_with_consent(
        &mut pipe,
        &auth,
        &config,
        AgentPipeConsentRuntime {
            consent: Some(&consent),
            context: Some(&context),
        },
        None,
    )?;

    assert_eq!(session.extension_origin, ORIGIN);
    assert_eq!(consent.calls(), 1);
    Ok(())
}

#[test]
fn consent_deny_releases_session_capacity() -> Result<(), Box<dyn std::error::Error>> {
    let auth = Arc::new(Mutex::new(auth(1)?));
    let config = config("session-1", "server-nonce-0001");
    let response = valid_response()?;
    let mut pipe = PipeIo::new(frames(&hello(), &response)?);
    let consent = MockConsent::new(AgentPipeConsentDecision::Denied {
        code: "resident_pipe.connection_consent_required",
        message: "resident pipe rejected native-host session: connection consent required",
    });
    let context = context();

    let result = authenticate_shared_server_pipe_with_consent(
        &mut pipe,
        &auth,
        &config,
        AgentPipeConsentRuntime {
            consent: Some(&consent),
            context: Some(&context),
        },
        None,
    );

    assert!(matches!(
        result,
        Err(AgentPipeSessionError::ConsentDenied { .. })
    ));
    assert!(
        auth.lock()
            .map_err(|_| PipeAuthError::InvalidResponse)?
            .issue_challenge(hello(), 2_000, 5_000, "session-2", "server-nonce-0002")
            .is_ok()
    );
    Ok(())
}

#[test]
fn handshake_timeout_includes_auth_and_consent_windows() {
    assert_eq!(
        handshake_timeout(5_000, 30_000),
        std::time::Duration::from_secs(35)
    );
}

fn auth(max_sessions: usize) -> Result<PipeServerAuth, PipeAuthError> {
    PipeServerAuth::new(
        "S-1-5-21-1000",
        "install-stable",
        &["chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"],
        Some(PipeSecret::new("per-install-secret")?),
        max_sessions,
        2,
    )
}

fn config(session_id: &str, server_nonce: &str) -> AgentPipeSessionConfig {
    AgentPipeSessionConfig {
        now_ms: 1_000,
        challenge_ttl_ms: 5_000,
        session_id: session_id.to_owned(),
        server_nonce: server_nonce.to_owned(),
    }
}

fn hello() -> BridgeHello {
    BridgeHello {
        current_user_sid: "S-1-5-21-1000".to_owned(),
        install_id: "install-stable".to_owned(),
        extension_origin: ORIGIN.to_owned(),
        client_nonce: "client-nonce-0001".to_owned(),
    }
}

const ORIGIN: &str = "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/";

fn valid_response() -> Result<BridgeAuthResponse, PipeAuthError> {
    let challenge = PipeChallenge {
        algorithm: "pipe-auth-v2-hmac-sha256".to_owned(),
        session_id: "session-1".to_owned(),
        install_id: "install-stable".to_owned(),
        client_nonce: "client-nonce-0001".to_owned(),
        server_nonce: "server-nonce-0001".to_owned(),
        expires_at_ms: 6_000,
    };
    build_auth_response(&challenge, &PipeSecret::new("per-install-secret")?)
}

fn tampered_session_response(
    challenge_session_id: &str,
    response_session_id: &str,
) -> Result<BridgeAuthResponse, PipeAuthError> {
    let challenge = PipeChallenge {
        algorithm: "pipe-auth-v2-hmac-sha256".to_owned(),
        session_id: challenge_session_id.to_owned(),
        install_id: "install-stable".to_owned(),
        client_nonce: "client-nonce-0001".to_owned(),
        server_nonce: "server-nonce-0001".to_owned(),
        expires_at_ms: 6_000,
    };
    let mut response = build_auth_response(&challenge, &PipeSecret::new("per-install-secret")?)?;
    response.session_id = response_session_id.to_owned();
    Ok(response)
}

fn frames(
    hello: &BridgeHello,
    response: &BridgeAuthResponse,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut input = Vec::new();
    let hello = serde_json::to_vec(hello)?;
    write_payload(&mut input, &hello, 16 * 1024)?;
    let response = serde_json::to_vec(response)?;
    write_payload(&mut input, &response, 16 * 1024)?;
    Ok(input)
}

struct PipeIo {
    reader: Cursor<Vec<u8>>,
    writer: Vec<u8>,
}

impl PipeIo {
    fn new(input: Vec<u8>) -> Self {
        Self {
            reader: Cursor::new(input),
            writer: Vec::new(),
        }
    }
}

impl Read for PipeIo {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.reader.read(buffer)
    }
}

impl Write for PipeIo {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.writer.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn context() -> AgentPipeConnectionContext {
    AgentPipeConnectionContext {
        host_exe_sha256: "1".repeat(64),
        parent_fingerprint: "2".repeat(64),
        parent_label: "chrome.exe".to_owned(),
        parent_category: "recognized-browser-parent",
    }
}

#[derive(Debug)]
struct MockConsent {
    decision: AgentPipeConsentDecision,
    requests: Mutex<Vec<AgentPipeConsentRequest>>,
}

impl MockConsent {
    fn new(decision: AgentPipeConsentDecision) -> Self {
        Self {
            decision,
            requests: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> usize {
        self.requests.lock().map_or(0, |requests| requests.len())
    }
}

impl AgentPipeConnectionConsent for MockConsent {
    fn confirm_connection(&self, request: &AgentPipeConsentRequest) -> AgentPipeConsentDecision {
        if let Ok(mut requests) = self.requests.lock() {
            requests.push(request.clone());
        }
        self.decision
    }
}
