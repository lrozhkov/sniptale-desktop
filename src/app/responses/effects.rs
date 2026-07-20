//! Native-host response side-effect boundary.

#[cfg(any(windows, test))]
use crate::capture::PendingScreenshotTransfer;
#[cfg(any(windows, test))]
use crate::protocol::{
    AppRecordingProgressMessage, AppRecordingStartedMessage, AppRecordingStoppedMessage,
    AppScreenshotStartMessage,
};
#[cfg(any(windows, test))]
use crate::transfer::chunks::EncodedChunk;
#[cfg(any(windows, test))]
use crate::{
    capture::ScreenshotCaptureRequest,
    protocol::{DeliveryRoute, OperationAuthority},
};
use crate::{
    hotkeys::HotkeyRegistrationPlan,
    protocol::{
        ExtensionRecordingControlCommandMessage, ExtensionRecordingStartCommandMessage,
        ExtensionScreenshotCaptureCommandMessage,
    },
    recording::{PendingRecordingTransfer, RecordingStartPolicy},
    tray::{TrayActionRegistry, TrayNoticeKind},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeHostSideEffectError {
    #[cfg(test)]
    CommandDispatch,
    #[cfg(any(windows, test))]
    HotkeyApply,
    #[cfg(any(windows, test))]
    TrayApply,
    #[cfg(test)]
    ScreenshotRegionCancelled,
    #[cfg(test)]
    ScreenshotInvalidSelection,
    #[cfg(any(windows, test))]
    ScreenshotTransferState,
    #[cfg(any(windows, test))]
    UnsupportedCapability,
    #[cfg(test)]
    RecordingDiskPressure,
    #[cfg(any(windows, test))]
    RecordingTransferState,
    RecordingRecovery,
    #[cfg(any(windows, test))]
    WorkerQueueFull,
    #[cfg(any(windows, test))]
    WorkerStopped,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(any(windows, test))]
pub(crate) enum NativeHostTransferChunkError {
    State,
    ChunkOutOfBounds,
    Replay,
    ProtectedMedia,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum NativeHostCommandOutcome {
    #[cfg(any(windows, test))]
    Accepted,
    #[cfg(test)]
    AcceptedWithEvents(Vec<NativeHostCommandEvent>),
    UnsupportedCapability,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(any(windows, test))]
pub(crate) enum NativeHostCommandEvent {
    ScreenshotStart(AppScreenshotStartMessage),
    RecordingStarted(AppRecordingStartedMessage),
    RecordingProgress(AppRecordingProgressMessage),
    RecordingStopped(AppRecordingStoppedMessage),
}

#[cfg(any(windows, test))]
impl NativeHostCommandEvent {
    pub(crate) fn controller_lease_id(&self) -> &str {
        match self {
            Self::ScreenshotStart(message) => &message.controller_lease_id,
            Self::RecordingStarted(message) => &message.controller_lease_id,
            Self::RecordingProgress(message) => &message.controller_lease_id,
            Self::RecordingStopped(message) => &message.controller_lease_id,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(any(windows, test))]
pub(crate) struct NativeHostCommandFailureEvent {
    pub(crate) controller_lease_id: Option<String>,
    pub(crate) operation: crate::protocol::OperationFailedOperation,
    pub(crate) operation_id: Option<String>,
    pub(crate) phase: &'static str,
    pub(crate) kind: crate::protocol::OperationalErrorKind,
    pub(crate) occurred_at_epoch_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(any(windows, test))]
pub(crate) struct NativeHostTransferChunk {
    pub(crate) chunk: EncodedChunk,
    pub(crate) is_complete: bool,
}

pub(crate) trait NativeHostSideEffects {
    fn apply_hotkey_plan(
        &mut self,
        plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError>;

    fn pending_recordings_for_reconnect(
        &mut self,
    ) -> Result<Vec<PendingRecordingTransfer>, NativeHostSideEffectError> {
        Ok(Vec::new())
    }

    #[cfg(any(windows, test))]
    fn pending_screenshots_for_reconnect(
        &mut self,
    ) -> Result<Vec<PendingScreenshotTransfer>, NativeHostSideEffectError> {
        Ok(Vec::new())
    }

    fn apply_tray_registry(
        &mut self,
        _registry: TrayActionRegistry,
    ) -> Result<(), NativeHostSideEffectError> {
        Ok(())
    }

    fn mark_tray_connection_connected(
        &mut self,
        _generation: u64,
    ) -> Result<(), NativeHostSideEffectError> {
        Ok(())
    }

    #[cfg(any(windows, test))]
    fn mark_tray_connection_disconnected(
        &mut self,
        _generation: u64,
    ) -> Result<(), NativeHostSideEffectError> {
        Ok(())
    }

    fn show_tray_notice(&mut self, _kind: TrayNoticeKind) -> Result<(), NativeHostSideEffectError> {
        Ok(())
    }

    fn ack_screenshot_transfer(
        &mut self,
        _capture_id: &str,
    ) -> Result<bool, NativeHostSideEffectError> {
        Ok(false)
    }

    fn reject_screenshot_transfer(
        &mut self,
        _capture_id: &str,
        _retain_for_diagnostics: bool,
    ) -> Result<bool, NativeHostSideEffectError> {
        Ok(false)
    }

    fn ack_recording_transfer(
        &mut self,
        _recording_id: &str,
    ) -> Result<bool, NativeHostSideEffectError> {
        Ok(false)
    }

    fn reject_recording_transfer(
        &mut self,
        _recording_id: &str,
        _retain_for_diagnostics: bool,
    ) -> Result<bool, NativeHostSideEffectError> {
        Ok(false)
    }

    #[cfg(any(windows, test))]
    fn screenshot_transfer_chunk(
        &mut self,
        _capture_id: &str,
        _chunk_index: u32,
        _now_ms: u64,
    ) -> Result<Option<NativeHostTransferChunk>, NativeHostTransferChunkError> {
        Ok(None)
    }

    #[cfg(any(windows, test))]
    fn recording_transfer_chunk(
        &mut self,
        _recording_id: &str,
        _chunk_index: u32,
        _now_ms: u64,
    ) -> Result<Option<EncodedChunk>, NativeHostTransferChunkError> {
        Ok(None)
    }

    fn start_screenshot_capture(
        &mut self,
        _command: &ExtensionScreenshotCaptureCommandMessage,
    ) -> Result<NativeHostCommandOutcome, NativeHostSideEffectError> {
        Ok(NativeHostCommandOutcome::UnsupportedCapability)
    }

    fn start_recording(
        &mut self,
        _command: &ExtensionRecordingStartCommandMessage,
        _policy: RecordingStartPolicy,
    ) -> Result<NativeHostCommandOutcome, NativeHostSideEffectError> {
        Ok(NativeHostCommandOutcome::UnsupportedCapability)
    }

    fn control_recording(
        &mut self,
        _command: &ExtensionRecordingControlCommandMessage,
    ) -> Result<NativeHostCommandOutcome, NativeHostSideEffectError> {
        Ok(NativeHostCommandOutcome::UnsupportedCapability)
    }
}

#[cfg(any(windows, test))]
pub(crate) fn extension_screenshot_capture_request(
    command: &ExtensionScreenshotCaptureCommandMessage,
) -> ScreenshotCaptureRequest {
    ScreenshotCaptureRequest {
        operation_id: command.command_id.clone(),
        mode: command.mode.clone(),
        authority: OperationAuthority::ExtensionManaged {
            settings_revision: command.settings_revision.clone(),
        },
        delivery_route: DeliveryRoute::ExtensionTransfer,
        include_cursor: true,
        open_editor: command.open_editor,
        requested_at_epoch_ms: command.requested_at_epoch_ms,
    }
}

#[derive(Default)]
#[cfg(any(not(windows), test))]
pub(crate) struct NoopNativeHostSideEffects;

#[cfg(any(not(windows), test))]
impl NativeHostSideEffects for NoopNativeHostSideEffects {
    fn apply_hotkey_plan(
        &mut self,
        _plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        Ok(())
    }
}
