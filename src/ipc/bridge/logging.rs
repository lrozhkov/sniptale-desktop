//! Native-host bridge forwarding diagnostics.

use std::sync::Arc;

use super::BridgeForwardError;
use crate::protocol::WireMessage;

pub(crate) trait BridgeForwardLog: Send + Sync {
    fn event(&self, phase: &'static str, message: &str, error_code: Option<&str>);
}

pub(crate) type BridgeForwardLogSink = Arc<dyn BridgeForwardLog>;

pub(super) fn log_bridge_event(
    log: Option<&BridgeForwardLogSink>,
    phase: &'static str,
    message: &str,
) {
    if let Some(log) = log {
        log.event(phase, message, None);
    }
}

pub(super) fn log_bridge_error(
    log: Option<&BridgeForwardLogSink>,
    phase: &'static str,
    error: &BridgeForwardError,
) {
    if let Some(log) = log {
        let code = error_code(error);
        let message = format!("native-host bridge forwarding failed code={code}");
        log.event(phase, &message, Some(code));
    }
}

#[cfg(any(windows, test))]
#[cfg(test)]
pub(super) const fn bridge_error_forward_phase(error: &BridgeForwardError) -> &'static str {
    match error {
        BridgeForwardError::BrowserFrame(_)
        | BridgeForwardError::BrowserProtocol(_)
        | BridgeForwardError::AgentFrame(_) => "browser-to-agent-forward-error",
        BridgeForwardError::AgentProtocol(_)
        | BridgeForwardError::AgentResponseHeader(_)
        | BridgeForwardError::AgentResponseMissing => "agent-to-browser-forward-error",
        BridgeForwardError::ForwardThread => "bridge-forward-thread-error",
    }
}

pub(super) const fn wire_message_type(message: &WireMessage) -> &'static str {
    match message {
        WireMessage::ExtensionHello(_) => "extension.hello",
        WireMessage::AppHello(_) => "app.hello",
        WireMessage::ExtensionControllerAcquire(_) => "extension.controller.acquire",
        WireMessage::AppControllerLease(_) => "app.controller.lease",
        WireMessage::ExtensionSettingsSync(_) => "extension.settings.sync",
        WireMessage::AppSettingsAccepted(_) => "app.settings.accepted",
        WireMessage::ExtensionPing(_) => "extension.ping",
        WireMessage::AppPong(_) => "app.pong",
        WireMessage::AppTrayActionRequested(_) => "app.tray.actionRequested",
        WireMessage::ExtensionTrayActionResult(_) => "extension.tray.actionResult",
        WireMessage::AppOpenSettingsRequested(_) => "app.openSettings.requested",
        WireMessage::ExtensionScreenshotCaptureCommand(_) => "extension.screenshot.capture",
        WireMessage::ExtensionRecordingStartCommand(_) => "extension.recording.start",
        WireMessage::ExtensionRecordingControlCommand(_) => "extension.recording.control",
        WireMessage::AppCommandAccepted(_) => "app.command.accepted",
        WireMessage::AppScreenshotStart(_) => "app.screenshot.start",
        WireMessage::AppScreenshotChunk(_) => "app.screenshot.chunk",
        WireMessage::AppScreenshotCommit(_) => "app.screenshot.commit",
        WireMessage::ExtensionScreenshotChunkRequest(_) => "extension.screenshot.chunkRequest",
        WireMessage::ExtensionScreenshotAck(_) => "extension.screenshot.ack",
        WireMessage::ExtensionScreenshotReject(_) => "extension.screenshot.reject",
        WireMessage::AppRecordingStarted(_) => "app.recording.started",
        WireMessage::AppRecordingProgress(_) => "app.recording.progress",
        WireMessage::AppRecordingStopped(_) => "app.recording.stopped",
        WireMessage::ExtensionRecordingChunkRequest(_) => "extension.recording.chunkRequest",
        WireMessage::AppRecordingChunk(_) => "app.recording.chunk",
        WireMessage::ExtensionRecordingAck(_) => "extension.recording.ack",
        WireMessage::ExtensionRecordingReject(_) => "extension.recording.reject",
        WireMessage::AppOperationFailed(_) => "app.operation.failed",
    }
}

fn error_code(error: &BridgeForwardError) -> &'static str {
    match error {
        BridgeForwardError::BrowserFrame(_) => "browser-frame",
        BridgeForwardError::AgentFrame(_) => "agent-frame",
        BridgeForwardError::BrowserProtocol(_) => "browser-protocol",
        BridgeForwardError::AgentProtocol(_) => "agent-protocol",
        BridgeForwardError::AgentResponseHeader(_) => "agent-response-header",
        BridgeForwardError::AgentResponseMissing => "agent-response-missing",
        BridgeForwardError::ForwardThread => "forward-thread",
    }
}
