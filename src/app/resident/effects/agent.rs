//! Native-host side-effect adapter composition.

use super::super::{
    capture_worker::CaptureWorkerCommandSink, hotkeys::HotkeyWorkerPlanSink,
    recording_worker::RecordingWorkerCommandSink, tray::TrayWorkerRegistrySink,
};
use crate::{
    app::responses::{
        NativeHostCommandOutcome, NativeHostSideEffectError, NativeHostSideEffects,
        NativeHostTransferChunk, NativeHostTransferChunkError,
    },
    capture::{PendingScreenshotTransfer, ScreenshotTransferStore},
    hotkeys::HotkeyRegistrationPlan,
    protocol::{
        ExtensionRecordingControlCommandMessage, ExtensionRecordingStartCommandMessage,
        ExtensionScreenshotCaptureCommandMessage,
    },
    recording::{PendingRecordingTransfer, RecordingStartPolicy, RecordingTransferStore},
    transfer::chunks::EncodedChunk,
    tray::{TrayActionRegistry, TrayNoticeKind},
};

pub(in crate::app::resident) struct AgentSideEffects {
    pub(super) hotkeys: HotkeyWorkerPlanSink,
    pub(super) tray: TrayWorkerRegistrySink,
    pub(super) screenshots: Option<ScreenshotTransferStore>,
    pub(super) capture: CaptureWorkerCommandSink,
    pub(super) recordings: Option<RecordingTransferStore>,
    pub(super) recording: RecordingWorkerCommandSink,
    active_settings: Option<crate::settings::ActiveSettings>,
    pub(super) browser_target_id: Option<String>,
    pub(super) assignments: Option<crate::transfer::browser_delivery::BrowserDeliveryStore>,
}

impl AgentSideEffects {
    #[cfg(test)]
    pub(in crate::app::resident) fn new(
        hotkeys: HotkeyWorkerPlanSink,
        tray: TrayWorkerRegistrySink,
        screenshots: Option<ScreenshotTransferStore>,
        capture: CaptureWorkerCommandSink,
        recordings: Option<RecordingTransferStore>,
        recording: RecordingWorkerCommandSink,
    ) -> Self {
        Self::new_with_active_settings(
            hotkeys,
            tray,
            screenshots,
            capture,
            recordings,
            recording,
            None,
        )
    }

    pub(in crate::app::resident) fn new_with_active_settings(
        hotkeys: HotkeyWorkerPlanSink,
        tray: TrayWorkerRegistrySink,
        screenshots: Option<ScreenshotTransferStore>,
        capture: CaptureWorkerCommandSink,
        recordings: Option<RecordingTransferStore>,
        recording: RecordingWorkerCommandSink,
        active_settings: Option<crate::settings::ActiveSettings>,
    ) -> Self {
        Self {
            hotkeys,
            tray,
            screenshots,
            capture,
            recordings,
            recording,
            active_settings,
            browser_target_id: None,
            assignments: None,
        }
    }

    fn extension_ingress_allowed(&self) -> bool {
        self.active_settings
            .as_ref()
            .is_none_or(crate::settings::ActiveSettings::allows_extension_ingress)
    }

    pub(super) fn assignment_store(
        &self,
    ) -> Result<crate::transfer::browser_delivery::BrowserDeliveryStore, NativeHostSideEffectError>
    {
        self.assignments
            .clone()
            .map_or_else(
                crate::transfer::browser_delivery::BrowserDeliveryStore::open_default,
                Ok,
            )
            .map_err(|_| NativeHostSideEffectError::RecordingTransferState)
    }
}

impl NativeHostSideEffects for AgentSideEffects {
    fn recording_attachment_manifest(
        &mut self,
        request: &crate::protocol::trajectory::ExtensionRecordingAttachmentManifestRequestMessage,
    ) -> Result<
        crate::protocol::trajectory::AppRecordingAttachmentManifestMessage,
        NativeHostSideEffectError,
    > {
        if !self.extension_ingress_allowed() {
            return Err(NativeHostSideEffectError::UnsupportedCapability);
        }
        self.recording_attachment_manifest_from_store(request)
    }

    fn recording_attachment_read_grant(
        &mut self,
        request: &crate::protocol::trajectory::ExtensionRecordingAttachmentChunkRequestMessage,
    ) -> Result<crate::protocol::trajectory::CursorTrajectoryReadGrant, NativeHostSideEffectError>
    {
        if !self.extension_ingress_allowed() {
            return Err(NativeHostSideEffectError::UnsupportedCapability);
        }
        self.recording_attachment_grant_from_store(request)
    }

    fn ack_recording_attachment(
        &mut self,
        ack: &crate::protocol::trajectory::ExtensionRecordingAttachmentAckMessage,
    ) -> Result<bool, NativeHostSideEffectError> {
        if !self.extension_ingress_allowed() {
            return Ok(false);
        }
        self.ack_recording_attachment_in_store(ack)
    }

    fn pending_recording_attachments(
        &mut self,
    ) -> Result<
        Vec<crate::transfer::trajectory_catalog::PendingTrajectory>,
        NativeHostSideEffectError,
    > {
        if !self.extension_ingress_allowed() {
            return Ok(Vec::new());
        }
        self.pending_recording_attachments_from_store()
    }
    fn recording_events_manifest(
        &mut self,
        request: &crate::protocol::recording_events::ExtensionRecordingEventsManifestRequestMessage,
    ) -> Result<
        crate::protocol::recording_events::AppRecordingEventsManifestMessage,
        NativeHostSideEffectError,
    > {
        if !self.extension_ingress_allowed() {
            return Err(NativeHostSideEffectError::UnsupportedCapability);
        }
        self.recording_events_manifest_from_store(request)
    }

    fn recording_events_read_grant(
        &mut self,
        request: &crate::protocol::recording_events::ExtensionRecordingEventsChunkRequestMessage,
    ) -> Result<
        crate::protocol::recording_events::RecordingEventsReadGrant,
        NativeHostSideEffectError,
    > {
        if !self.extension_ingress_allowed() {
            return Err(NativeHostSideEffectError::UnsupportedCapability);
        }
        self.recording_events_grant_from_store(request)
    }

    fn ack_recording_events(
        &mut self,
        ack: &crate::protocol::recording_events::ExtensionRecordingEventsAckMessage,
    ) -> Result<bool, NativeHostSideEffectError> {
        if !self.extension_ingress_allowed() {
            return Ok(false);
        }
        self.ack_recording_events_in_store(ack)
    }

    fn pending_recording_event_attachments(
        &mut self,
    ) -> Result<Vec<crate::transfer::events_catalog::PendingEvents>, NativeHostSideEffectError>
    {
        if !self.extension_ingress_allowed() {
            return Ok(Vec::new());
        }
        self.pending_recording_event_attachments_from_store()
    }
    fn bind_browser_target(&mut self, target_id: &str) {
        self.browser_target_id = Some(target_id.to_owned());
    }
    fn apply_hotkey_plan(
        &mut self,
        plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        if !self.extension_ingress_allowed() {
            return Err(NativeHostSideEffectError::UnsupportedCapability);
        }
        self.apply_hotkey_plan_to_worker(plan)
    }

    fn apply_tray_registry(
        &mut self,
        registry: TrayActionRegistry,
    ) -> Result<(), NativeHostSideEffectError> {
        if !self.extension_ingress_allowed() {
            return Err(NativeHostSideEffectError::UnsupportedCapability);
        }
        self.apply_tray_registry_to_worker(registry)
    }

    fn show_tray_notice(&mut self, kind: TrayNoticeKind) -> Result<(), NativeHostSideEffectError> {
        if !self.extension_ingress_allowed() {
            return Err(NativeHostSideEffectError::UnsupportedCapability);
        }
        self.show_tray_notice_in_worker(kind)
    }

    #[cfg(any(windows, test))]
    fn mark_tray_connection_connected(
        &mut self,
        generation: u64,
    ) -> Result<(), NativeHostSideEffectError> {
        self.mark_tray_connection_connected_in_worker(generation)
    }

    #[cfg(any(windows, test))]
    fn mark_tray_connection_disconnected(
        &mut self,
        generation: u64,
    ) -> Result<(), NativeHostSideEffectError> {
        self.mark_tray_connection_disconnected_in_worker(generation)
    }

    fn pending_recordings_for_reconnect(
        &mut self,
    ) -> Result<Vec<PendingRecordingTransfer>, NativeHostSideEffectError> {
        if !self.extension_ingress_allowed() {
            return Ok(Vec::new());
        }
        self.pending_recordings_for_reconnect_from_store()
    }

    fn pending_screenshots_for_reconnect(
        &mut self,
    ) -> Result<Vec<PendingScreenshotTransfer>, NativeHostSideEffectError> {
        if !self.extension_ingress_allowed() {
            return Ok(Vec::new());
        }
        self.pending_screenshots_for_reconnect_from_store()
    }

    fn ack_screenshot_transfer(
        &mut self,
        capture_id: &str,
    ) -> Result<bool, NativeHostSideEffectError> {
        if !self.extension_ingress_allowed() {
            return Ok(false);
        }
        self.ack_screenshot_transfer_in_store(capture_id)
    }

    fn reject_screenshot_transfer(
        &mut self,
        capture_id: &str,
        retain_for_diagnostics: bool,
    ) -> Result<bool, NativeHostSideEffectError> {
        if !self.extension_ingress_allowed() {
            return Ok(false);
        }
        self.reject_screenshot_transfer_in_store(capture_id, retain_for_diagnostics)
    }

    fn screenshot_transfer_chunk(
        &mut self,
        capture_id: &str,
        chunk_index: u32,
        now_ms: u64,
    ) -> Result<Option<NativeHostTransferChunk>, NativeHostTransferChunkError> {
        if !self.extension_ingress_allowed() {
            return Err(NativeHostTransferChunkError::State);
        }
        self.screenshot_transfer_chunk_from_store(capture_id, chunk_index, now_ms)
    }

    fn start_screenshot_capture(
        &mut self,
        command: &ExtensionScreenshotCaptureCommandMessage,
    ) -> Result<NativeHostCommandOutcome, NativeHostSideEffectError> {
        if !self.extension_ingress_allowed() {
            return Err(NativeHostSideEffectError::UnsupportedCapability);
        }
        self.start_screenshot_capture_worker(command)
    }

    fn ack_recording_transfer(
        &mut self,
        recording_id: &str,
    ) -> Result<bool, NativeHostSideEffectError> {
        if !self.extension_ingress_allowed() {
            return Ok(false);
        }
        self.ack_recording_transfer_in_store(recording_id)
    }

    fn reject_recording_transfer(
        &mut self,
        recording_id: &str,
        retain_for_diagnostics: bool,
    ) -> Result<bool, NativeHostSideEffectError> {
        if !self.extension_ingress_allowed() {
            return Ok(false);
        }
        self.reject_recording_transfer_in_store(recording_id, retain_for_diagnostics)
    }

    fn recording_transfer_chunk(
        &mut self,
        recording_id: &str,
        chunk_index: u32,
        now_ms: u64,
    ) -> Result<Option<EncodedChunk>, NativeHostTransferChunkError> {
        if !self.extension_ingress_allowed() {
            return Err(NativeHostTransferChunkError::State);
        }
        self.recording_transfer_chunk_from_store(recording_id, chunk_index, now_ms)
    }

    fn start_recording(
        &mut self,
        command: &ExtensionRecordingStartCommandMessage,
        policy: RecordingStartPolicy,
    ) -> Result<NativeHostCommandOutcome, NativeHostSideEffectError> {
        if !self.extension_ingress_allowed() {
            return Err(NativeHostSideEffectError::UnsupportedCapability);
        }
        self.start_recording_worker(command, policy)
    }

    fn control_recording(
        &mut self,
        command: &ExtensionRecordingControlCommandMessage,
    ) -> Result<NativeHostCommandOutcome, NativeHostSideEffectError> {
        if !self.extension_ingress_allowed() {
            return Err(NativeHostSideEffectError::UnsupportedCapability);
        }
        self.control_recording_worker(command)
    }
}
