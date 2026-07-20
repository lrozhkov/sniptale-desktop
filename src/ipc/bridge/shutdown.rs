//! Shutdown-aware bridge error classification.

use super::BridgeForwardError;

pub(super) fn is_expected_shutdown_forward_error(error: &BridgeForwardError) -> bool {
    match error {
        BridgeForwardError::AgentFrame(error) | BridgeForwardError::BrowserFrame(error) => {
            error.is_closed_transport()
        }
        BridgeForwardError::BrowserProtocol(_)
        | BridgeForwardError::AgentProtocol(_)
        | BridgeForwardError::AgentResponseHeader(_)
        | BridgeForwardError::AgentResponseMissing
        | BridgeForwardError::ForwardThread => false,
    }
}
