//! Privacy-safe labels for connection-consent UI.

pub(super) const CONSENT_STORE_UNAVAILABLE_MESSAGE: &str =
    "resident pipe rejected session: trust store unavailable";

pub(crate) fn safe_extension_label(origin: &str) -> String {
    const PREFIX: &str = "chrome-extension://";
    let Some(rest) = origin.strip_prefix(PREFIX) else {
        return "extension:unknown".to_owned();
    };
    let id = rest.trim_end_matches('/');
    if id.len() == 32 && id.bytes().all(|byte| byte.is_ascii_lowercase()) {
        format!("extension:{}...", &id[..8])
    } else {
        "extension:invalid".to_owned()
    }
}
