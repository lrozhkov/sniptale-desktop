//! Current-user authenticated named-pipe server.

use super::auth_mac::{PIPE_AUTH_ALGORITHM, verify_challenge_response_hash};
pub(crate) use super::server_types::{
    AuthenticatedPipeSession, BridgeAuthResponse, BridgeHello, PipeAuthError, PipeChallenge,
    PipeSecret,
};

const USED_SERVER_NONCE_LIMIT: usize = 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
struct PendingChallenge {
    challenge: PipeChallenge,
    current_user_sid: String,
    extension_origin: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PipeServerAuth {
    expected_user_sid: String,
    install_id: String,
    expected_extension_origins: Vec<String>,
    secret: Option<PipeSecret>,
    max_sessions: usize,
    queue_capacity: usize,
    pending: Vec<PendingChallenge>,
    sessions: Vec<AuthenticatedPipeSession>,
    used_server_nonces: Vec<String>,
}

impl PipeSecret {
    #[cfg(any(windows, test))]
    pub(crate) fn new(secret: &str) -> Result<Self, PipeAuthError> {
        if secret.is_empty() {
            Err(PipeAuthError::MissingSecret)
        } else {
            Ok(Self(secret.to_owned()))
        }
    }

    pub(crate) fn material(&self) -> &str {
        &self.0
    }
}

impl PipeServerAuth {
    #[cfg(any(windows, test))]
    pub(crate) fn new(
        expected_user_sid: &str,
        install_id: &str,
        expected_extension_origins: &[&str],
        secret: Option<PipeSecret>,
        max_sessions: usize,
        queue_capacity: usize,
    ) -> Result<Self, PipeAuthError> {
        if secret.is_none() {
            return Err(PipeAuthError::MissingSecret);
        }
        if max_sessions == 0 || queue_capacity == 0 {
            return Err(PipeAuthError::SessionLimit);
        }
        Ok(Self {
            expected_user_sid: expected_user_sid.to_owned(),
            install_id: install_id.to_owned(),
            expected_extension_origins: expected_extension_origins
                .iter()
                .map(|origin| (*origin).to_owned())
                .collect(),
            secret,
            max_sessions,
            queue_capacity,
            pending: Vec::new(),
            sessions: Vec::new(),
            used_server_nonces: Vec::new(),
        })
    }

    pub(crate) fn issue_challenge(
        &mut self,
        hello: BridgeHello,
        now_ms: u64,
        ttl_ms: u64,
        session_id: &str,
        server_nonce: &str,
    ) -> Result<PipeChallenge, PipeAuthError> {
        self.validate_capacity()?;
        validate_nonce(&hello.client_nonce)?;
        validate_nonce(server_nonce)?;
        if hello.current_user_sid != self.expected_user_sid {
            return Err(PipeAuthError::WrongUser);
        }
        if hello.install_id != self.install_id {
            return Err(PipeAuthError::WrongInstall);
        }
        if !self
            .expected_extension_origins
            .iter()
            .any(|origin| origin == &hello.extension_origin)
        {
            return Err(PipeAuthError::WrongOrigin);
        }
        let challenge = PipeChallenge {
            algorithm: PIPE_AUTH_ALGORITHM.to_owned(),
            session_id: session_id.to_owned(),
            install_id: self.install_id.clone(),
            client_nonce: hello.client_nonce,
            server_nonce: server_nonce.to_owned(),
            expires_at_ms: now_ms.saturating_add(ttl_ms),
        };
        self.pending.push(PendingChallenge {
            challenge: challenge.clone(),
            current_user_sid: hello.current_user_sid,
            extension_origin: hello.extension_origin,
        });
        Ok(challenge)
    }

    pub(crate) fn authenticate(
        &mut self,
        response: BridgeAuthResponse,
        now_ms: u64,
    ) -> Result<AuthenticatedPipeSession, PipeAuthError> {
        let pending_index = self.pending_index(&response)?;
        let pending = self.pending.remove(pending_index);
        Self::validate_pending(&pending, &response, now_ms)?;
        self.validate_response(&pending.challenge, &response)?;
        let session = AuthenticatedPipeSession {
            session_id: pending.challenge.session_id,
            current_user_sid: pending.current_user_sid,
            extension_origin: pending.extension_origin,
            queue_capacity: self.queue_capacity,
            queued_messages: 0,
        };
        self.remember_server_nonce(response.server_nonce);
        self.sessions.push(session.clone());
        Ok(session)
    }

    pub(crate) fn finish_session(&mut self, session_id: &str) {
        self.sessions
            .retain(|session| session.session_id != session_id);
    }

    pub(crate) fn discard_challenge(&mut self, session_id: &str) {
        self.pending
            .retain(|pending| pending.challenge.session_id != session_id);
    }

    pub(crate) fn max_sessions(&self) -> usize {
        self.max_sessions
    }

    #[cfg(test)]
    pub(crate) fn used_server_nonce_count(&self) -> usize {
        self.used_server_nonces.len()
    }

    fn validate_capacity(&self) -> Result<(), PipeAuthError> {
        if self.pending.len() + self.sessions.len() >= self.max_sessions {
            Err(PipeAuthError::SessionLimit)
        } else {
            Ok(())
        }
    }

    fn pending_index(&self, response: &BridgeAuthResponse) -> Result<usize, PipeAuthError> {
        if self.used_server_nonces.contains(&response.server_nonce) {
            return Err(PipeAuthError::ReplayedChallenge);
        }
        self.pending
            .iter()
            .position(|pending| pending.challenge.session_id == response.session_id)
            .ok_or(PipeAuthError::InvalidResponse)
    }

    fn validate_pending(
        pending: &PendingChallenge,
        response: &BridgeAuthResponse,
        now_ms: u64,
    ) -> Result<(), PipeAuthError> {
        if now_ms > pending.challenge.expires_at_ms {
            return Err(PipeAuthError::ExpiredChallenge);
        }
        if response.client_nonce != pending.challenge.client_nonce
            || response.server_nonce != pending.challenge.server_nonce
        {
            return Err(PipeAuthError::InvalidNonce);
        }
        Ok(())
    }

    fn validate_response(
        &self,
        challenge: &PipeChallenge,
        response: &BridgeAuthResponse,
    ) -> Result<(), PipeAuthError> {
        if response.algorithm != challenge.algorithm {
            return Err(PipeAuthError::InvalidResponse);
        }
        let secret = self.secret.as_ref().ok_or(PipeAuthError::MissingSecret)?;
        verify_challenge_response_hash(secret, &self.install_id, challenge, &response.response_hash)
    }

    fn remember_server_nonce(&mut self, server_nonce: String) {
        if self.used_server_nonces.len() >= USED_SERVER_NONCE_LIMIT {
            self.used_server_nonces.remove(0);
        }
        self.used_server_nonces.push(server_nonce);
    }
}

impl AuthenticatedPipeSession {
    pub(crate) fn enqueue_forwarded_message(&mut self) -> Result<(), PipeAuthError> {
        if self.queued_messages >= self.queue_capacity {
            return Err(PipeAuthError::QueueFull);
        }
        self.queued_messages += 1;
        Ok(())
    }

    pub(crate) fn complete_forwarded_message(&mut self) {
        self.queued_messages = self.queued_messages.saturating_sub(1);
    }
}

fn validate_nonce(nonce: &str) -> Result<(), PipeAuthError> {
    if nonce.len() < 16 {
        Err(PipeAuthError::InvalidNonce)
    } else {
        Ok(())
    }
}
