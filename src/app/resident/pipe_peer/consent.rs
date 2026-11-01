//! User consent and persisted trust for resident pipe connections.

mod labels;

use labels::CONSENT_STORE_UNAVAILABLE_MESSAGE;
pub(crate) use labels::safe_extension_label;

use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;

mod store;
use store::{TrustedConnectionKey, TrustedConnectionStore, load_trust_store, save_trust_store};

use crate::app::bridge::session::AgentPipeConsentRequest;
#[cfg(windows)]
use crate::app::bridge::session::{AgentPipeConnectionConsent, AgentPipeConsentDecision};
#[cfg(windows)]
use crate::tray::consent::WindowsConnectionConsentPrompt;
use crate::tray::{
    ConnectionConsentPrompt, ConnectionConsentPromptRequest, ConnectionConsentPromptResult,
};

#[cfg(windows)]
const TRUST_FILE_RELATIVE_PATH: &[&str] = &["Sniptale", "NativeAgent", "trusted-connections.json"];
const TRUST_FORMAT_VERSION: u8 = 1;
const CONSENT_DENIED_CODE: &str = "resident_pipe.connection_consent_required";
const CONSENT_DENIED_MESSAGE: &str = "resident pipe rejected session: connection consent required";
const CONSENT_STORE_CORRUPT_CODE: &str = "resident_pipe.connection_consent_store_corrupt";
const CONSENT_STORE_CORRUPT_MESSAGE: &str =
    "resident pipe rejected session: trust store is invalid";
const CONSENT_STORE_UNAVAILABLE_CODE: &str = "resident_pipe.connection_consent_store_unavailable";
#[cfg(windows)]
const CONSENT_STORE_WRITE_FAILED_CODE: &str = "resident_pipe.connection_consent_store_write_failed";
#[cfg(windows)]
const CONSENT_STORE_WRITE_FAILED_MESSAGE: &str =
    "resident pipe accepted native-host session, but trusted connection was not persisted";

#[derive(Debug)]
pub(super) enum ConnectionTrustStoreError {
    Io(std::io::Error),
    Json,
    UnsupportedFormat,
}

#[cfg(windows)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::app::resident) struct WindowsConnectionConsent<P> {
    store_path: PathBuf,
    prompt: P,
    timeout_ms: u64,
    allow_persisted_trust: bool,
}

#[cfg(windows)]
impl<P> WindowsConnectionConsent<P> {
    pub(super) const fn new(
        store_path: PathBuf,
        prompt: P,
        timeout_ms: u64,
        allow_persisted_trust: bool,
    ) -> Self {
        Self {
            store_path,
            prompt,
            timeout_ms,
            allow_persisted_trust,
        }
    }
}

#[cfg(windows)]
impl WindowsConnectionConsent<WindowsConnectionConsentPrompt> {
    fn default() -> Option<Self> {
        Some(Self::new(
            trusted_connections_file_path()?,
            WindowsConnectionConsentPrompt,
            30_000,
            matches!(
                crate::install::expected_install_identity().channel,
                crate::install::InstallChannel::Dev
            ) && crate::install::dev_local_browser_trust_enabled(),
        ))
    }
}

#[cfg(windows)]
#[derive(Debug)]
pub(crate) enum ResidentConnectionConsent {
    Store(WindowsConnectionConsent<WindowsConnectionConsentPrompt>),
    Unavailable,
}

#[cfg(windows)]
impl ResidentConnectionConsent {
    pub(crate) fn default() -> Self {
        WindowsConnectionConsent::default().map_or(Self::Unavailable, Self::Store)
    }
}

#[cfg(windows)]
impl AgentPipeConnectionConsent for ResidentConnectionConsent {
    fn confirm_connection(&self, request: &AgentPipeConsentRequest) -> AgentPipeConsentDecision {
        match self {
            Self::Store(store) => store.confirm_connection(request),
            Self::Unavailable => AgentPipeConsentDecision::Denied {
                code: CONSENT_DENIED_CODE,
                message: CONSENT_DENIED_MESSAGE,
            },
        }
    }

    fn timeout_ms(&self) -> u64 {
        match self {
            Self::Store(store) => store.timeout_ms(),
            Self::Unavailable => 30_000,
        }
    }
}

#[cfg(windows)]
impl<P> AgentPipeConnectionConsent for WindowsConnectionConsent<P>
where
    P: ConnectionConsentPrompt,
{
    fn confirm_connection(&self, request: &AgentPipeConsentRequest) -> AgentPipeConsentDecision {
        if self.allow_persisted_trust {
            return confirm_connection_core(
                &self.store_path,
                &self.prompt,
                self.timeout_ms,
                request,
                true,
                None,
            )
            .into();
        }
        let identity = crate::install::expected_install_identity();
        let Ok(secret) = crate::install::load_current_user_pipe_secret(identity.install_id) else {
            return AgentPipeConsentDecision::Denied {
                code: CONSENT_STORE_UNAVAILABLE_CODE,
                message: CONSENT_STORE_UNAVAILABLE_MESSAGE,
            };
        };
        confirm_connection_core(
            &self.store_path,
            &self.prompt,
            self.timeout_ms,
            request,
            true,
            Some(secret.secret_material.as_bytes()),
        )
        .into()
    }

    fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }
}

#[cfg(test)]
pub(crate) fn confirm_connection_with_store(
    path: &Path,
    prompt: &dyn ConnectionConsentPrompt,
    timeout_ms: u64,
    request: &AgentPipeConsentRequest,
    allow_persisted_trust: bool,
) -> ConnectionConsentStoreDecision {
    confirm_connection_core(
        path,
        prompt,
        timeout_ms,
        request,
        allow_persisted_trust,
        None,
    )
}

fn confirm_connection_core(
    path: &Path,
    prompt: &dyn ConnectionConsentPrompt,
    timeout_ms: u64,
    request: &AgentPipeConsentRequest,
    allow_persisted_trust: bool,
    authentication_key: Option<&[u8]>,
) -> ConnectionConsentStoreDecision {
    let key = TrustedConnectionKey::from_request(request);
    let mut store = if allow_persisted_trust {
        match load_trust_store(path, authentication_key) {
            Ok(store) => store,
            Err(ConnectionTrustStoreError::Io(error))
                if error.kind() == std::io::ErrorKind::NotFound =>
            {
                TrustedConnectionStore::default()
            }
            Err(ConnectionTrustStoreError::Json | ConnectionTrustStoreError::UnsupportedFormat) => {
                return ConnectionConsentStoreDecision::Denied {
                    code: CONSENT_STORE_CORRUPT_CODE,
                    message: CONSENT_STORE_CORRUPT_MESSAGE,
                };
            }
            Err(ConnectionTrustStoreError::Io(_)) => {
                return ConnectionConsentStoreDecision::Denied {
                    code: CONSENT_STORE_UNAVAILABLE_CODE,
                    message: CONSENT_STORE_UNAVAILABLE_MESSAGE,
                };
            }
        }
    } else {
        TrustedConnectionStore::default()
    };
    if allow_persisted_trust && store.contains(&key) {
        return ConnectionConsentStoreDecision::Allowed;
    }
    let prompt_request = ConnectionConsentPromptRequest {
        extension_label: safe_extension_label(&request.extension_origin),
        parent_label: request.connection.parent_label.clone(),
        parent_category: request.connection.parent_category,
        timeout_ms,
    };
    match prompt.prompt(&prompt_request) {
        ConnectionConsentPromptResult::Allow => {
            if !allow_persisted_trust {
                return ConnectionConsentStoreDecision::Allowed;
            }
            store.insert(key);
            if save_trust_store(path, &store, authentication_key).is_err() {
                return ConnectionConsentStoreDecision::AllowedWithoutPersist;
            }
            ConnectionConsentStoreDecision::Allowed
        }
        ConnectionConsentPromptResult::Deny => ConnectionConsentStoreDecision::Denied {
            code: CONSENT_DENIED_CODE,
            message: CONSENT_DENIED_MESSAGE,
        },
        #[cfg(any(windows, test))]
        ConnectionConsentPromptResult::Timeout | ConnectionConsentPromptResult::UiUnavailable => {
            ConnectionConsentStoreDecision::Denied {
                code: CONSENT_DENIED_CODE,
                message: CONSENT_DENIED_MESSAGE,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConnectionConsentStoreDecision {
    Allowed,
    AllowedWithoutPersist,
    Denied {
        code: &'static str,
        message: &'static str,
    },
}

#[cfg(windows)]
impl From<ConnectionConsentStoreDecision> for AgentPipeConsentDecision {
    fn from(decision: ConnectionConsentStoreDecision) -> Self {
        match decision {
            ConnectionConsentStoreDecision::Allowed => Self::Allowed,
            ConnectionConsentStoreDecision::AllowedWithoutPersist => Self::AllowedWithWarning {
                code: CONSENT_STORE_WRITE_FAILED_CODE,
                message: CONSENT_STORE_WRITE_FAILED_MESSAGE,
            },
            ConnectionConsentStoreDecision::Denied { code, message } => {
                Self::Denied { code, message }
            }
        }
    }
}

#[cfg(windows)]
fn trusted_connections_file_path() -> Option<PathBuf> {
    let mut path = PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    for segment in TRUST_FILE_RELATIVE_PATH {
        path.push(segment);
    }
    Some(path)
}
