//! Bounded consent state parsing and authenticated atomic persistence.

use super::{AgentPipeConsentRequest, ConnectionTrustStoreError, TRUST_FORMAT_VERSION};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    path::Path,
};

pub(super) fn load_trust_store(
    path: &Path,
    authentication_key: Option<&[u8]>,
) -> Result<TrustedConnectionStore, ConnectionTrustStoreError> {
    let mut payload = Vec::new();
    std::fs::File::open(path)
        .map_err(ConnectionTrustStoreError::Io)?
        .take(128 * 1024 + 1)
        .read_to_end(&mut payload)
        .map_err(ConnectionTrustStoreError::Io)?;
    if payload.len() > 128 * 1024 {
        return Err(ConnectionTrustStoreError::Json);
    }
    if let Some(key) = authentication_key {
        payload = crate::ipc::open_persisted_trust(&payload, key)
            .map_err(|_| ConnectionTrustStoreError::Json)?;
    }
    let store: TrustedConnectionStore =
        serde_json::from_slice(&payload).map_err(|_| ConnectionTrustStoreError::Json)?;
    if store.format_version == TRUST_FORMAT_VERSION && store.trusted.len() <= 32 {
        Ok(store)
    } else {
        Err(ConnectionTrustStoreError::UnsupportedFormat)
    }
}

pub(super) fn save_trust_store(
    path: &Path,
    store: &TrustedConnectionStore,
    authentication_key: Option<&[u8]>,
) -> Result<(), ConnectionTrustStoreError> {
    let payload = serde_json::to_vec_pretty(store).map_err(|_| ConnectionTrustStoreError::Json)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(ConnectionTrustStoreError::Io)?;
    }
    let payload = if let Some(key) = authentication_key {
        crate::ipc::seal_persisted_trust(&payload, key)
            .map_err(|_| ConnectionTrustStoreError::Json)?
    } else {
        payload
    };
    let pending = path.with_extension("pending");
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&pending)
        .map_err(ConnectionTrustStoreError::Io)?;
    if let Err(error) = file
        .write_all(&payload)
        .and_then(|()| file.sync_all())
        .and_then(|()| std::fs::rename(&pending, path))
    {
        let _ = std::fs::remove_file(&pending);
        return Err(ConnectionTrustStoreError::Io(error));
    }
    Ok(())
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TrustedConnectionStore {
    format_version: u8,
    trusted: Vec<TrustedConnectionKey>,
}

impl TrustedConnectionStore {
    pub(super) fn contains(&self, key: &TrustedConnectionKey) -> bool {
        self.trusted.iter().any(|trusted| trusted == key)
    }

    pub(super) fn insert(&mut self, key: TrustedConnectionKey) {
        self.format_version = TRUST_FORMAT_VERSION;
        if !self.contains(&key) {
            self.trusted.push(key);
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct TrustedConnectionKey {
    extension_origin: String,
    host_exe_sha256: String,
    parent_fingerprint: String,
}

impl TrustedConnectionKey {
    pub(super) fn from_request(request: &AgentPipeConsentRequest) -> Self {
        Self {
            extension_origin: request.extension_origin.clone(),
            host_exe_sha256: request.connection.host_exe_sha256.clone(),
            parent_fingerprint: request.connection.parent_fingerprint.clone(),
        }
    }
}
