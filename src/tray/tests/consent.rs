use crate::tray::{
    ConnectionConsentPrompt, ConnectionConsentPromptRequest, ConnectionConsentPromptResult,
};

#[derive(Clone, Copy)]
struct StaticConsentPrompt(ConnectionConsentPromptResult);

impl ConnectionConsentPrompt for StaticConsentPrompt {
    fn prompt(&self, request: &ConnectionConsentPromptRequest) -> ConnectionConsentPromptResult {
        assert_eq!(request.extension_label, "Sniptale Extension");
        assert_eq!(request.parent_label, "Chrome");
        assert_eq!(request.parent_category, "browser");
        assert_eq!(request.timeout_ms, 30_000);
        self.0
    }
}

fn consent_request() -> ConnectionConsentPromptRequest {
    ConnectionConsentPromptRequest {
        extension_label: "Sniptale Extension".to_owned(),
        parent_label: "Chrome".to_owned(),
        parent_category: "browser",
        timeout_ms: 30_000,
    }
}

#[test]
fn connection_consent_prompt_returns_typed_decision() {
    let prompt = StaticConsentPrompt(ConnectionConsentPromptResult::Allow);

    assert_eq!(
        prompt.prompt(&consent_request()),
        ConnectionConsentPromptResult::Allow
    );
}

#[test]
fn connection_consent_prompt_timeout_stays_distinct() {
    let prompt = StaticConsentPrompt(ConnectionConsentPromptResult::Timeout);

    assert_eq!(
        prompt.prompt(&consent_request()),
        ConnectionConsentPromptResult::Timeout
    );
}
