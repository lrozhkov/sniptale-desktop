//! Native-host ping response owner.

use serde_json::Value;

use super::{NativeHostSession, outbound::pong};

pub(super) fn ping_responses(
    session: &mut NativeHostSession,
    nonce: &str,
    sent_at_epoch_ms: u64,
) -> Vec<Value> {
    session.trusted_epoch_ms(sent_at_epoch_ms);
    vec![pong(nonce, sent_at_epoch_ms, session.settings_revision())]
}
