//! Typed native consent prompt boundary.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConnectionConsentPromptResult {
    Allow,
    Deny,
    #[cfg(any(windows, test))]
    Timeout,
    #[cfg(any(windows, test))]
    UiUnavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ConnectionConsentPromptRequest {
    pub(crate) extension_label: String,
    pub(crate) parent_label: String,
    pub(crate) parent_category: &'static str,
    pub(crate) timeout_ms: u64,
}

pub(crate) trait ConnectionConsentPrompt: Sync {
    fn prompt(&self, request: &ConnectionConsentPromptRequest) -> ConnectionConsentPromptResult;
}
