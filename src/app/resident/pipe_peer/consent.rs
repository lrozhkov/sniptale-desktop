//! User consent and persisted trust for resident pipe connections.

mod labels;

use labels::CONSENT_STORE_UNAVAILABLE_MESSAGE;
pub(crate) use labels::safe_extension_label;

use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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
        confirm_connection_with_store(
            &self.store_path,
            &self.prompt,
            self.timeout_ms,
            request,
            self.allow_persisted_trust,
        )
        .into()
    }

    fn timeout_ms(&self) -> u64 {
        self.timeout_ms
    }
}

pub(crate) fn confirm_connection_with_store(
    path: &Path,
    prompt: &dyn ConnectionConsentPrompt,
    timeout_ms: u64,
    request: &AgentPipeConsentRequest,
    allow_persisted_trust: bool,
) -> ConnectionConsentStoreDecision {
    let key = TrustedConnectionKey::from_request(request);
    let mut store = if allow_persisted_trust {
        match load_trust_store(path) {
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
            if save_trust_store(path, &store).is_err() {
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

fn load_trust_store(path: &Path) -> Result<TrustedConnectionStore, ConnectionTrustStoreError> {
    let payload = std::fs::read(path).map_err(ConnectionTrustStoreError::Io)?;
    let store: TrustedConnectionStore =
        serde_json::from_slice(&payload).map_err(|_| ConnectionTrustStoreError::Json)?;
    if store.format_version == TRUST_FORMAT_VERSION {
        Ok(store)
    } else {
        Err(ConnectionTrustStoreError::UnsupportedFormat)
    }
}

fn save_trust_store(
    path: &Path,
    store: &TrustedConnectionStore,
) -> Result<(), ConnectionTrustStoreError> {
    let payload = serde_json::to_vec_pretty(store).map_err(|_| ConnectionTrustStoreError::Json)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(ConnectionTrustStoreError::Io)?;
    }
    std::fs::write(path, payload).map_err(ConnectionTrustStoreError::Io)
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrustedConnectionStore {
    format_version: u8,
    trusted: Vec<TrustedConnectionKey>,
}

impl TrustedConnectionStore {
    fn contains(&self, key: &TrustedConnectionKey) -> bool {
        self.trusted.iter().any(|trusted| trusted == key)
    }

    fn insert(&mut self, key: TrustedConnectionKey) {
        self.format_version = TRUST_FORMAT_VERSION;
        if !self.contains(&key) {
            self.trusted.push(key);
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrustedConnectionKey {
    extension_origin: String,
    host_exe_sha256: String,
    parent_fingerprint: String,
}

impl TrustedConnectionKey {
    fn from_request(request: &AgentPipeConsentRequest) -> Self {
        Self {
            extension_origin: request.extension_origin.clone(),
            host_exe_sha256: request.connection.host_exe_sha256.clone(),
            parent_fingerprint: request.connection.parent_fingerprint.clone(),
        }
    }
}
