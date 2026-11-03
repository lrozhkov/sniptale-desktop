//! Native-host ping response owner.

use serde_json::Value;

use super::{NativeHostSession, outbound::pong};

pub(super) fn ping_responses(
    session: &mut NativeHostSession,
    nonce: &str,
    sent_at_epoch_ms: u64,
    effects: &mut impl super::NativeHostSideEffects,
) -> Vec<Value> {
    session.trusted_epoch_ms(sent_at_epoch_ms);
    let mut responses = Vec::new();
    #[cfg(any(windows, test))]
    if session.desktop_owned() {
        responses = super::desktop_settings::reconcile_selection(session, effects);
    }
    #[cfg(not(any(windows, test)))]
    let _ = effects;
    responses.push(pong(nonce, sent_at_epoch_ms, session.settings_revision()));
    responses
}
