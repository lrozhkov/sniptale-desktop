//! Application-owned connection consent. Unavailable UI and timeout never approve.
#![cfg(windows)]

use super::prompt::{
    ConnectionConsentPrompt, ConnectionConsentPromptRequest, ConnectionConsentPromptResult,
};
use crate::tray::desktop::{prompts::PromptContent, show_prompt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WindowsConnectionConsentPrompt;

impl ConnectionConsentPrompt for WindowsConnectionConsentPrompt {
    fn prompt(&self, request: &ConnectionConsentPromptRequest) -> ConnectionConsentPromptResult {
        match show_prompt(
            PromptContent::Connection {
                extension: request.extension_label.clone(),
                launcher: request.parent_label.clone(),
                category: request.parent_category.to_owned(),
            },
            std::time::Duration::from_millis(request.timeout_ms),
        ) {
            Some(1) => ConnectionConsentPromptResult::Allow,
            Some(0) => ConnectionConsentPromptResult::Deny,
            Some(_) => ConnectionConsentPromptResult::UiUnavailable,
            None => ConnectionConsentPromptResult::Timeout,
        }
    }
}
