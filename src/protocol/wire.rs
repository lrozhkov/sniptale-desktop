use serde::{Deserialize, Serialize};

use super::{
    control::controller::{AppControllerLeaseMessage, ExtensionControllerAcquireMessage},
    control::handshake::{AppHelloMessage, ExtensionHelloMessage},
    control::media_commands::{
        AppCommandAcceptedMessage, ExtensionRecordingControlCommandMessage,
        ExtensionRecordingStartCommandMessage, ExtensionScreenshotCaptureCommandMessage,
    },
    control::settings::{AppSettingsAcceptedMessage, ExtensionSettingsSyncMessage},
    control::status::{AppPongMessage, ExtensionPingMessage},
    control::tray::{
        AppOpenSettingsRequestedMessage, AppTrayActionRequestedMessage,
        ExtensionTrayActionResultMessage,
    },
    media::failure::AppOperationFailedMessage,
    media::recording::{
        AppRecordingChunkMessage, AppRecordingProgressMessage, AppRecordingStartedMessage,
        AppRecordingStoppedMessage, ExtensionRecordingAckMessage,
        ExtensionRecordingChunkRequestMessage, ExtensionRecordingRejectMessage,
    },
    media::screenshot::{
        AppScreenshotChunkMessage, AppScreenshotCommitMessage, AppScreenshotStartMessage,
        ExtensionScreenshotAckMessage, ExtensionScreenshotChunkRequestMessage,
        ExtensionScreenshotRejectMessage,
    },
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "type")]
pub(crate) enum WireMessage {
    #[serde(rename = "extension.hello")]
    ExtensionHello(Box<ExtensionHelloMessage>),
    #[serde(rename = "app.hello")]
    AppHello(Box<AppHelloMessage>),
    #[serde(rename = "extension.controller.acquire")]
    ExtensionControllerAcquire(Box<ExtensionControllerAcquireMessage>),
    #[serde(rename = "app.controller.lease")]
    AppControllerLease(Box<AppControllerLeaseMessage>),
    #[serde(rename = "extension.settings.sync")]
    ExtensionSettingsSync(Box<ExtensionSettingsSyncMessage>),
    #[serde(rename = "app.settings.accepted")]
    AppSettingsAccepted(Box<AppSettingsAcceptedMessage>),
    #[serde(rename = "extension.ping")]
    ExtensionPing(Box<ExtensionPingMessage>),
    #[serde(rename = "app.pong")]
    AppPong(Box<AppPongMessage>),
    #[serde(rename = "app.tray.actionRequested")]
    AppTrayActionRequested(Box<AppTrayActionRequestedMessage>),
    #[serde(rename = "extension.tray.actionResult")]
    ExtensionTrayActionResult(Box<ExtensionTrayActionResultMessage>),
    #[serde(rename = "app.openSettings.requested")]
    AppOpenSettingsRequested(Box<AppOpenSettingsRequestedMessage>),
    #[serde(rename = "extension.screenshot.capture")]
    ExtensionScreenshotCaptureCommand(Box<ExtensionScreenshotCaptureCommandMessage>),
    #[serde(rename = "extension.recording.start")]
    ExtensionRecordingStartCommand(Box<ExtensionRecordingStartCommandMessage>),
    #[serde(rename = "extension.recording.control")]
    ExtensionRecordingControlCommand(Box<ExtensionRecordingControlCommandMessage>),
    #[serde(rename = "app.command.accepted")]
    AppCommandAccepted(Box<AppCommandAcceptedMessage>),
    #[serde(rename = "app.screenshot.start")]
    AppScreenshotStart(Box<AppScreenshotStartMessage>),
    #[serde(rename = "app.screenshot.chunk")]
    AppScreenshotChunk(Box<AppScreenshotChunkMessage>),
    #[serde(rename = "app.screenshot.commit")]
    AppScreenshotCommit(Box<AppScreenshotCommitMessage>),
    #[serde(rename = "extension.screenshot.chunkRequest")]
    ExtensionScreenshotChunkRequest(Box<ExtensionScreenshotChunkRequestMessage>),
    #[serde(rename = "extension.screenshot.ack")]
    ExtensionScreenshotAck(Box<ExtensionScreenshotAckMessage>),
    #[serde(rename = "extension.screenshot.reject")]
    ExtensionScreenshotReject(Box<ExtensionScreenshotRejectMessage>),
    #[serde(rename = "app.recording.started")]
    AppRecordingStarted(Box<AppRecordingStartedMessage>),
    #[serde(rename = "app.recording.progress")]
    AppRecordingProgress(Box<AppRecordingProgressMessage>),
    #[serde(rename = "app.recording.stopped")]
    AppRecordingStopped(Box<AppRecordingStoppedMessage>),
    #[serde(rename = "extension.recording.chunkRequest")]
    ExtensionRecordingChunkRequest(Box<ExtensionRecordingChunkRequestMessage>),
    #[serde(rename = "app.recording.chunk")]
    AppRecordingChunk(Box<AppRecordingChunkMessage>),
    #[serde(rename = "extension.recording.ack")]
    ExtensionRecordingAck(Box<ExtensionRecordingAckMessage>),
    #[serde(rename = "extension.recording.reject")]
    ExtensionRecordingReject(Box<ExtensionRecordingRejectMessage>),
    #[serde(rename = "app.operation.failed")]
    AppOperationFailed(Box<AppOperationFailedMessage>),
}
