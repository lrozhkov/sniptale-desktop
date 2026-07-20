use std::sync::atomic::{AtomicUsize, Ordering};

use crate::{
    app::bridge::session::{AgentPipeConnectionContext, AgentPipeConsentRequest},
    tray::{
        ConnectionConsentPrompt, ConnectionConsentPromptRequest, ConnectionConsentPromptResult,
    },
};

use super::consent::{ConnectionConsentStoreDecision, confirm_connection_with_store};

const ORIGIN: &str = "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/";
const HOST_HASH: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const PARENT_FINGERPRINT: &str = "2222222222222222222222222222222222222222222222222222222222222222";

#[test]
fn production_policy_never_uses_or_writes_user_forgeable_persisted_trust()
-> Result<(), Box<dyn std::error::Error>> {
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("trusted-connections.json");
    let forged = format!(
        r#"{{"formatVersion":1,"trusted":[{{"extensionOrigin":"{ORIGIN}","hostExeSha256":"{HOST_HASH}","parentFingerprint":"{PARENT_FINGERPRINT}"}}]}}"#,
    );
    std::fs::write(&path, &forged)?;
    let prompt = CountingAllowPrompt::default();
    let request = AgentPipeConsentRequest {
        extension_origin: ORIGIN.to_owned(),
        connection: AgentPipeConnectionContext {
            host_exe_sha256: HOST_HASH.to_owned(),
            parent_fingerprint: PARENT_FINGERPRINT.to_owned(),
            parent_label: "chrome.exe".to_owned(),
            parent_category: "recognized-browser-parent",
        },
    };

    for _ in 0..2 {
        assert_eq!(
            confirm_connection_with_store(&path, &prompt, 30_000, &request, false),
            ConnectionConsentStoreDecision::Allowed
        );
    }
    assert_eq!(prompt.calls.load(Ordering::Relaxed), 2);
    assert_eq!(std::fs::read_to_string(path)?, forged);
    Ok(())
}

#[derive(Debug, Default)]
struct CountingAllowPrompt {
    calls: AtomicUsize,
}

impl ConnectionConsentPrompt for CountingAllowPrompt {
    fn prompt(&self, _request: &ConnectionConsentPromptRequest) -> ConnectionConsentPromptResult {
        self.calls.fetch_add(1, Ordering::Relaxed);
        ConnectionConsentPromptResult::Allow
    }
}
