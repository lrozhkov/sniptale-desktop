//! Media worker command dispatch side effects.

use super::agent::AgentSideEffects;
use crate::{
    app::responses::{
        NativeHostCommandOutcome, NativeHostSideEffectError, extension_screenshot_capture_request,
    },
    protocol::{
        ExtensionRecordingControlCommandMessage, ExtensionRecordingStartCommandMessage,
        ExtensionScreenshotCaptureCommandMessage,
    },
    recording::RecordingStartPolicy,
};

impl AgentSideEffects {
    pub(super) fn start_screenshot_capture_worker(
        &mut self,
        command: &ExtensionScreenshotCaptureCommandMessage,
    ) -> Result<NativeHostCommandOutcome, NativeHostSideEffectError> {
        self.capture.start_screenshot_capture(
            extension_screenshot_capture_request(command),
            Some(&command.controller_lease_id),
        )
    }

    pub(super) fn start_recording_worker(
        &mut self,
        command: &ExtensionRecordingStartCommandMessage,
        policy: RecordingStartPolicy,
    ) -> Result<NativeHostCommandOutcome, NativeHostSideEffectError> {
        self.recording.start_recording(command, policy)
    }

    pub(super) fn control_recording_worker(
        &mut self,
        command: &ExtensionRecordingControlCommandMessage,
    ) -> Result<NativeHostCommandOutcome, NativeHostSideEffectError> {
        self.recording.control_recording(command)
    }
}
