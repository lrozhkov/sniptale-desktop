use crate::ipc::{BridgeHello, PipeAuthError, PipeSecret, PipeServerAuth, build_auth_response};
use proptest::{prelude::*, test_runner::TestCaseError};

const USER_SID: &str = "S-1-5-21-1000";
const INSTALL_ID: &str = "install-stable";
const SECRET: &str = "per-install-secret";
const ORIGIN: &str = "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/";
const CLIENT_NONCE: &str = "client-nonce-0001";
const SERVER_NONCE: &str = "server-nonce-0001";

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

#[test]
fn valid_bridge_challenge_authenticates_current_user_session() -> Result<(), PipeAuthError> {
    let mut server = server(2, 2)?;
    let challenge = server.issue_challenge(hello(), 1_000, 5_000, "session-1", SERVER_NONCE)?;
    let response = build_auth_response(&challenge, &secret()?)?;

    let session = server.authenticate(response, 2_000)?;

    assert_eq!(session.session_id, "session-1");
    assert_eq!(session.current_user_sid, USER_SID);
    Ok(())
}

#[test]
fn missing_or_wrong_secret_fails_closed() -> Result<(), PipeAuthError> {
    assert!(matches!(
        PipeServerAuth::new(USER_SID, INSTALL_ID, &[ORIGIN], None, 1, 1),
        Err(PipeAuthError::MissingSecret)
    ));

    let mut server = server(1, 1)?;
    let challenge = server.issue_challenge(hello(), 1_000, 5_000, "session-1", SERVER_NONCE)?;
    let wrong_secret = PipeSecret::new("wrong-secret")?;
    let response = build_auth_response(&challenge, &wrong_secret)?;

    assert_eq!(
        server.authenticate(response, 2_000),
        Err(PipeAuthError::InvalidResponse)
    );
    Ok(())
}

#[test]
fn wrong_user_or_install_never_receives_challenge() -> Result<(), PipeAuthError> {
    let mut server = server(1, 1)?;
    let mut wrong_user = hello();
    wrong_user.current_user_sid = "S-1-5-21-2000".to_owned();
    assert_eq!(
        server.issue_challenge(wrong_user, 1_000, 5_000, "session-1", SERVER_NONCE),
        Err(PipeAuthError::WrongUser)
    );

    let mut wrong_install = hello();
    wrong_install.install_id = "install-dev".to_owned();
    assert_eq!(
        server.issue_challenge(wrong_install, 1_000, 5_000, "session-1", SERVER_NONCE),
        Err(PipeAuthError::WrongInstall)
    );
    let mut wrong_origin = hello();
    wrong_origin.extension_origin =
        "chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/".to_owned();
    assert_eq!(
        server.issue_challenge(wrong_origin, 1_000, 5_000, "session-1", SERVER_NONCE),
        Err(PipeAuthError::WrongOrigin)
    );
    Ok(())
}

#[test]
fn replayed_or_expired_challenge_is_rejected() -> Result<(), PipeAuthError> {
    let mut server = server(2, 1)?;
    let challenge = server.issue_challenge(hello(), 1_000, 5_000, "session-1", SERVER_NONCE)?;
    let response = build_auth_response(&challenge, &secret()?)?;
    server.authenticate(response.clone(), 2_000)?;

    assert_eq!(
        server.authenticate(response, 2_001),
        Err(PipeAuthError::ReplayedChallenge)
    );

    let challenge =
        server.issue_challenge(hello(), 10_000, 5_000, "session-2", "server-nonce-0002")?;
    let response = build_auth_response(&challenge, &secret()?)?;
    assert_eq!(
        server.authenticate(response, 15_001),
        Err(PipeAuthError::ExpiredChallenge)
    );
    Ok(())
}

#[test]
fn used_server_nonce_replay_cache_is_bounded() -> Result<(), PipeAuthError> {
    let mut server = server(1, 1)?;

    for index in 0..1_100 {
        let session_id = format!("session-{index}");
        let server_nonce = format!("server-nonce-{index:04}");
        let challenge =
            server.issue_challenge(hello(), 1_000, 5_000, &session_id, &server_nonce)?;
        let session = server.authenticate(build_auth_response(&challenge, &secret()?)?, 2_000)?;
        server.finish_session(&session.session_id);
    }

    assert_eq!(server.used_server_nonce_count(), 1024);
    Ok(())
}

#[test]
fn session_limit_and_queue_backpressure_are_bounded() -> Result<(), PipeAuthError> {
    let mut server = server(1, 1)?;
    let challenge = server.issue_challenge(hello(), 1_000, 5_000, "session-1", SERVER_NONCE)?;
    assert_eq!(
        server.issue_challenge(hello(), 1_000, 5_000, "session-2", "server-nonce-0002"),
        Err(PipeAuthError::SessionLimit)
    );

    let mut session = server.authenticate(build_auth_response(&challenge, &secret()?)?, 2_000)?;
    assert_eq!(
        server.issue_challenge(hello(), 2_000, 5_000, "session-2", "server-nonce-0002"),
        Err(PipeAuthError::SessionLimit)
    );
    server.finish_session(&session.session_id);
    assert!(
        server
            .issue_challenge(hello(), 2_000, 5_000, "session-2", "server-nonce-0002")
            .is_ok()
    );

    session.enqueue_forwarded_message()?;
    assert_eq!(
        session.enqueue_forwarded_message(),
        Err(PipeAuthError::QueueFull)
    );
    session.complete_forwarded_message();
    assert!(session.enqueue_forwarded_message().is_ok());
    Ok(())
}

#[test]
fn tampered_algorithm_nonce_or_expiry_is_rejected() -> Result<(), PipeAuthError> {
    let mut server = server(3, 1)?;
    let challenge = server.issue_challenge(hello(), 1_000, 5_000, "session-1", SERVER_NONCE)?;

    let mut algorithm_mismatch = build_auth_response(&challenge, &secret()?)?;
    algorithm_mismatch.algorithm = "pipe-auth-v1-sha256".to_owned();
    assert_eq!(
        server.authenticate(algorithm_mismatch, 2_000),
        Err(PipeAuthError::InvalidResponse)
    );

    let challenge =
        server.issue_challenge(hello(), 2_000, 5_000, "session-2", "server-nonce-0002")?;
    let mut nonce_mismatch = build_auth_response(&challenge, &secret()?)?;
    nonce_mismatch.server_nonce = "server-nonce-tampered".to_owned();
    assert_eq!(
        server.authenticate(nonce_mismatch, 3_000),
        Err(PipeAuthError::InvalidNonce)
    );

    let mut challenge =
        server.issue_challenge(hello(), 3_000, 5_000, "session-3", "server-nonce-0003")?;
    challenge.expires_at_ms = challenge.expires_at_ms.saturating_add(1);
    let forged_expiry_response = build_auth_response(&challenge, &secret()?)?;
    assert_eq!(
        server.authenticate(forged_expiry_response, 4_000),
        Err(PipeAuthError::InvalidResponse)
    );
    Ok(())
}

proptest! {
    #[test]
    fn pipe_auth_rejects_tampered_handshake_fields(tamper_case in 0_u8..=3) {
        let mut server = server(4, 1)
            .map_err(|error| TestCaseError::fail(format!("{error:?}")))?;
        let challenge = server
            .issue_challenge(hello(), 1_000, 5_000, "session-1", SERVER_NONCE)
            .map_err(|error| TestCaseError::fail(format!("{error:?}")))?;
        let secret = secret().map_err(|error| TestCaseError::fail(format!("{error:?}")))?;
        let mut response = build_auth_response(&challenge, &secret)
            .map_err(|error| TestCaseError::fail(format!("{error:?}")))?;

        let expected = match tamper_case {
            0 => {
                response.algorithm.push_str("-tampered");
                PipeAuthError::InvalidResponse
            }
            1 => {
                response.session_id.push_str("-tampered");
                PipeAuthError::InvalidResponse
            }
            2 => {
                response.client_nonce.push_str("-tampered");
                PipeAuthError::InvalidNonce
            }
            _ => {
                response.server_nonce.push_str("-tampered");
                PipeAuthError::InvalidNonce
            }
        };

        prop_assert_eq!(server.authenticate(response, 2_000), Err(expected));
    }
}

#[test]
fn malformed_response_hash_is_rejected() -> Result<(), PipeAuthError> {
    let mut server = server(1, 1)?;
    let challenge = server.issue_challenge(hello(), 1_000, 5_000, "session-1", SERVER_NONCE)?;
    let mut response = build_auth_response(&challenge, &secret()?)?;
    response.response_hash = "not-a-hex-hmac".to_owned();

    assert_eq!(
        server.authenticate(response, 2_000),
        Err(PipeAuthError::InvalidResponse)
    );
    Ok(())
}

#[test]
fn discarded_malformed_response_session_releases_capacity() -> Result<(), PipeAuthError> {
    let mut server = server(1, 1)?;
    let challenge = server.issue_challenge(hello(), 1_000, 5_000, "session-1", SERVER_NONCE)?;
    let mut response = build_auth_response(&challenge, &secret()?)?;
    response.session_id = "session-tampered".to_owned();

    assert_eq!(
        server.authenticate(response, 2_000),
        Err(PipeAuthError::InvalidResponse)
    );
    server.discard_challenge("session-1");

    assert!(
        server
            .issue_challenge(hello(), 2_000, 5_000, "session-2", "server-nonce-0002")
            .is_ok()
    );
    Ok(())
}
