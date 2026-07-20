use std::sync::Mutex;

use crate::app::bridge::session::{AgentPipeConnectionContext, AgentPipeConsentRequest};
use crate::tray::{
    ConnectionConsentPrompt, ConnectionConsentPromptRequest, ConnectionConsentPromptResult,
};

use super::consent::{
    ConnectionConsentStoreDecision, confirm_connection_with_store, safe_extension_label,
};

const ORIGIN: &str = "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/";
const HOST_HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const PARENT_FINGERPRINT: &str = "2222222222222222222222222222222222222222222222222222222222222222";

#[test]
fn allow_persists_trust_for_origin_hash_and_parent_fingerprint()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("trusted-connections.json");
    let prompt = MockPrompt::new(ConnectionConsentPromptResult::Allow);
    let request = request(ORIGIN, HOST_HASH, PARENT_FINGERPRINT);

    assert_eq!(
        confirm_connection_with_store(&path, &prompt, 30_000, &request, true),
        ConnectionConsentStoreDecision::Allowed
    );
    assert_eq!(
        confirm_connection_with_store(&path, &prompt, 30_000, &request, true),
        ConnectionConsentStoreDecision::Allowed
    );

    assert_eq!(prompt.calls(), 1);
    let payload = std::fs::read_to_string(path)?;
    assert!(payload.contains(ORIGIN));
    assert!(payload.contains(HOST_HASH));
    assert!(payload.contains(PARENT_FINGERPRINT));
    assert!(!payload.contains("pid"));
    assert!(!payload.contains("pipe-auth-v2"));
    Ok(())
}

#[test]
fn allow_without_store_write_still_accepts_current_session()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("trusted-connections.json");
    let prompt = WriteFailPrompt::new(path.clone());
    let request = request(ORIGIN, HOST_HASH, PARENT_FINGERPRINT);

    assert_eq!(
        confirm_connection_with_store(&path, &prompt, 30_000, &request, true),
        ConnectionConsentStoreDecision::AllowedWithoutPersist
    );
    assert_eq!(prompt.calls(), 1);
    Ok(())
}

#[test]
fn changed_origin_hash_or_fingerprint_requires_new_approval()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("trusted-connections.json");
    let prompt = MockPrompt::new(ConnectionConsentPromptResult::Allow);

    for request in [
        request(ORIGIN, HOST_HASH, PARENT_FINGERPRINT),
        request(
            "chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/",
            HOST_HASH,
            PARENT_FINGERPRINT,
        ),
        request(
            ORIGIN,
            "3333333333333333333333333333333333333333333333333333333333333333",
            PARENT_FINGERPRINT,
        ),
        request(
            ORIGIN,
            HOST_HASH,
            "4444444444444444444444444444444444444444444444444444444444444444",
        ),
    ] {
        assert_eq!(
            confirm_connection_with_store(&path, &prompt, 30_000, &request, true),
            ConnectionConsentStoreDecision::Allowed
        );
    }

    assert_eq!(prompt.calls(), 4);
    Ok(())
}

#[test]
fn deny_timeout_or_unavailable_do_not_persist_trust() -> Result<(), Box<dyn std::error::Error>> {
    for result in [
        ConnectionConsentPromptResult::Deny,
        ConnectionConsentPromptResult::Timeout,
        ConnectionConsentPromptResult::UiUnavailable,
    ] {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("trusted-connections.json");
        let prompt = MockPrompt::new(result);

        assert!(matches!(
            confirm_connection_with_store(
                &path,
                &prompt,
                30_000,
                &request(ORIGIN, HOST_HASH, PARENT_FINGERPRINT),
                true,
            ),
            ConnectionConsentStoreDecision::Denied { .. }
        ));
        assert!(!path.exists());
    }
    Ok(())
}

#[test]
fn corrupt_store_fails_closed_without_overwrite() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("trusted-connections.json");
    std::fs::write(&path, "{not-json")?;
    let prompt = MockPrompt::new(ConnectionConsentPromptResult::Allow);

    assert_eq!(
        confirm_connection_with_store(
            &path,
            &prompt,
            30_000,
            &request(ORIGIN, HOST_HASH, PARENT_FINGERPRINT),
            true,
        ),
        ConnectionConsentStoreDecision::Denied {
            code: "resident_pipe.connection_consent_store_corrupt",
            message: "resident pipe rejected session: trust store is invalid",
        }
    );
    assert_eq!(std::fs::read_to_string(path)?, "{not-json");
    assert_eq!(prompt.calls(), 0);
    Ok(())
}

#[test]
fn prompt_request_uses_safe_labels_only() -> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("trusted-connections.json");
    let prompt = MockPrompt::new(ConnectionConsentPromptResult::Allow);

    assert_eq!(
        confirm_connection_with_store(
            &path,
            &prompt,
            30_000,
            &request(ORIGIN, HOST_HASH, PARENT_FINGERPRINT),
            true,
        ),
        ConnectionConsentStoreDecision::Allowed
    );

    let last = prompt.last_request();
    assert_eq!(last.extension_label, "extension:aaaaaaaa...");
    assert_eq!(last.parent_label, "chrome.exe");
    assert_eq!(last.parent_category, "recognized-browser-parent");
    assert_eq!(last.timeout_ms, 30_000);
    assert!(!last.extension_label.contains("chrome-extension://"));
    assert!(!last.parent_label.contains('\\'));
    Ok(())
}

#[test]
fn extension_label_rejects_unexpected_origin_shape() {
    assert_eq!(
        safe_extension_label("https://example.test/"),
        "extension:unknown"
    );
    assert_eq!(
        safe_extension_label("chrome-extension://ABC/"),
        "extension:invalid"
    );
}

fn request(
    extension_origin: &str,
    host_exe_sha256: &str,
    parent_fingerprint: &str,
) -> AgentPipeConsentRequest {
    AgentPipeConsentRequest {
        extension_origin: extension_origin.to_owned(),
        connection: AgentPipeConnectionContext {
            host_exe_sha256: host_exe_sha256.to_owned(),
            parent_fingerprint: parent_fingerprint.to_owned(),
            parent_label: "chrome.exe".to_owned(),
            parent_category: "recognized-browser-parent",
        },
    }
}

#[derive(Debug)]
struct MockPrompt {
    result: ConnectionConsentPromptResult,
    requests: Mutex<Vec<ConnectionConsentPromptRequest>>,
}

impl MockPrompt {
    fn new(result: ConnectionConsentPromptResult) -> Self {
        Self {
            result,
            requests: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> usize {
        self.requests.lock().map_or(0, |requests| requests.len())
    }

    fn last_request(&self) -> ConnectionConsentPromptRequest {
        self.requests
            .lock()
            .map(|requests| requests.last().cloned())
            .ok()
            .flatten()
            .unwrap_or_else(|| ConnectionConsentPromptRequest {
                extension_label: String::new(),
                parent_label: String::new(),
                parent_category: "missing",
                timeout_ms: 0,
            })
    }
}

impl ConnectionConsentPrompt for MockPrompt {
    fn prompt(&self, request: &ConnectionConsentPromptRequest) -> ConnectionConsentPromptResult {
        if let Ok(mut requests) = self.requests.lock() {
            requests.push(request.clone());
        }
        self.result
    }
}

#[derive(Debug)]
struct WriteFailPrompt {
    path: std::path::PathBuf,
    requests: Mutex<Vec<ConnectionConsentPromptRequest>>,
}

impl WriteFailPrompt {
    fn new(path: std::path::PathBuf) -> Self {
        Self {
            path,
            requests: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> usize {
        self.requests.lock().map_or(0, |requests| requests.len())
    }
}

impl ConnectionConsentPrompt for WriteFailPrompt {
    fn prompt(&self, request: &ConnectionConsentPromptRequest) -> ConnectionConsentPromptResult {
        if let Ok(mut requests) = self.requests.lock() {
            requests.push(request.clone());
        }
        if std::fs::create_dir(&self.path).is_ok() {
            ConnectionConsentPromptResult::Allow
        } else {
            ConnectionConsentPromptResult::UiUnavailable
        }
    }
}
