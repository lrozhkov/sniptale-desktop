//! Tray-owned Manual operation admission after durable asset binding.

use std::{
    sync::mpsc::{self, SyncSender, TrySendError},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use crate::{
    app::resident::{WorkerCommand, control::manual_binding},
    assets::AssetFormat,
    capture::ScreenshotCaptureRequest,
    protocol::{RecordingControl, ScreenshotMode, VideoMode},
    recording::RecordingControlRequest,
    settings::ActiveSettings,
    transfer::identity::derived_media_id,
};

const ASSET_PREPARE_TIMEOUT: Duration = Duration::from_millis(500);

#[cfg(test)]
#[path = "manual_dispatch/tests.rs"]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManualDispatchError {
    ModeUnavailable,
    WorkerUnavailable,
    WorkerBusy,
    AssetPrepareFailed,
    EntropyUnavailable,
}

#[derive(Debug)]
pub(crate) struct ManualActionDispatcher {
    settings: ActiveSettings,
    capture: Option<SyncSender<WorkerCommand>>,
    recording: Option<SyncSender<WorkerCommand>>,
    assets: Option<SyncSender<WorkerCommand>>,
    sequence: u64,
    active_recording_id: Option<String>,
}

impl ManualActionDispatcher {
    pub(crate) fn new(
        settings: ActiveSettings,
        capture: Option<SyncSender<WorkerCommand>>,
        recording: Option<SyncSender<WorkerCommand>>,
        assets: Option<SyncSender<WorkerCommand>>,
    ) -> Self {
        Self {
            settings,
            capture,
            recording,
            assets,
            sequence: 0,
            active_recording_id: None,
        }
    }

    pub(crate) fn enqueue_screenshot(
        &mut self,
        mode: ScreenshotMode,
    ) -> Result<(), ManualDispatchError> {
        let operation_id = self.next_operation_id("screenshot")?;
        let binding =
            manual_binding::bind_manual_operation(&self.settings, &operation_id, AssetFormat::Png)
                .map_err(map_binding_error)?;
        let include_cursor = self
            .settings
            .profile_snapshot()
            .map_err(|_| ManualDispatchError::ModeUnavailable)?
            .manual_screenshot_include_cursor()
            .ok_or(ManualDispatchError::ModeUnavailable)?;
        self.prepare_asset(binding.job.clone())?;
        let request = ScreenshotCaptureRequest {
            operation_id,
            mode,
            authority: binding.authority,
            delivery_route: binding.delivery_route,
            include_cursor,
            open_editor: false,
            requested_at_epoch_ms: now_epoch_ms(),
        };
        send_worker(
            self.capture.as_ref(),
            WorkerCommand::CaptureScreenshot {
                request: Box::new(request),
                extension_controller_lease_id: None,
            },
        )
    }

    pub(crate) fn enqueue_recording(&mut self, mode: VideoMode) -> Result<(), ManualDispatchError> {
        let operation_id = self.next_operation_id("recording")?;
        let requested_at_epoch_ms = now_epoch_ms();
        let (binding, request, policy) = manual_binding::bind_manual_recording(
            &self.settings,
            &operation_id,
            mode,
            requested_at_epoch_ms,
        )
        .map_err(map_binding_error)?;
        let recording_id = derived_media_id("recording", &operation_id);
        self.prepare_asset(binding.job.clone())?;
        send_worker(
            self.recording.as_ref(),
            WorkerCommand::StartRecording {
                request: Box::new(request),
                policy,
            },
        )?;
        self.active_recording_id = Some(recording_id);
        Ok(())
    }

    pub(crate) fn control_recording(
        &mut self,
        control: &RecordingControl,
    ) -> Result<(), ManualDispatchError> {
        let recording_id = self
            .active_recording_id
            .clone()
            .ok_or(ManualDispatchError::WorkerUnavailable)?;
        let operation_id = self.next_operation_id("recording-control")?;
        send_worker(
            self.recording.as_ref(),
            WorkerCommand::ControlRecording {
                request: Box::new(RecordingControlRequest {
                    operation_id,
                    recording_id,
                    control: control.clone(),
                    requested_at_epoch_ms: now_epoch_ms(),
                    delivery_session: crate::recording::RecordingDeliverySession::default(),
                }),
            },
        )?;
        if matches!(control, RecordingControl::Stop | RecordingControl::Cancel) {
            self.active_recording_id = None;
        }
        Ok(())
    }

    fn prepare_asset(&self, job: crate::assets::AssetJob) -> Result<(), ManualDispatchError> {
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        send_worker(
            self.assets.as_ref(),
            WorkerCommand::PrepareManualAsset {
                job: Box::new(job),
                result_sender,
            },
        )?;
        match result_receiver.recv_timeout(ASSET_PREPARE_TIMEOUT) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(_)) | Err(_) => Err(ManualDispatchError::AssetPrepareFailed),
        }
    }

    fn next_operation_id(&mut self, kind: &str) -> Result<String, ManualDispatchError> {
        self.sequence = self.sequence.saturating_add(1);
        let mut nonce = [0_u8; 16];
        getrandom::fill(&mut nonce).map_err(|_| ManualDispatchError::EntropyUnavailable)?;
        Ok(derived_media_id(
            "manual",
            &format!("{kind}:{}:{}:{nonce:x?}", now_epoch_ms(), self.sequence),
        ))
    }
}

fn map_binding_error(_error: manual_binding::ManualOperationBindError) -> ManualDispatchError {
    ManualDispatchError::ModeUnavailable
}

fn send_worker(
    sender: Option<&SyncSender<WorkerCommand>>,
    command: WorkerCommand,
) -> Result<(), ManualDispatchError> {
    let sender = sender.ok_or(ManualDispatchError::WorkerUnavailable)?;
    match sender.try_send(command) {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(_)) => Err(ManualDispatchError::WorkerBusy),
        Err(TrySendError::Disconnected(_)) => Err(ManualDispatchError::WorkerUnavailable),
    }
}

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |duration| {
            duration.as_millis().try_into().unwrap_or(u64::MAX)
        })
        .max(1)
}
