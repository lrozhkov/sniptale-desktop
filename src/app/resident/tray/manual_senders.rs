//! Bounded worker ports used by Manual tray dispatch and mode fencing.

#[cfg(any(windows, test))]
use std::fmt;
use std::sync::mpsc::SyncSender;
#[cfg(any(windows, test))]
use std::{collections::BTreeMap, sync::mpsc};

use super::{ManualActionDispatcher, WorkerCommand};

#[cfg(any(windows, test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ManualHotkeyApplyError {
    ProfileUnavailable,
    SenderUnavailable,
    InvalidBindings,
    CommandQueueFull,
    CommandQueueDisconnected,
    AckTimedOut,
    AckDisconnected,
    Runtime(crate::hotkeys::HotkeyRuntimeError),
}

#[cfg(any(windows, test))]
impl fmt::Display for ManualHotkeyApplyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProfileUnavailable => formatter.write_str("manual hotkey profile unavailable"),
            Self::SenderUnavailable => formatter.write_str("manual hotkey sender unavailable"),
            Self::InvalidBindings => formatter.write_str("manual hotkey bindings invalid"),
            Self::CommandQueueFull => formatter.write_str("manual hotkey command queue full"),
            Self::CommandQueueDisconnected => {
                formatter.write_str("manual hotkey command queue disconnected")
            }
            Self::AckTimedOut => formatter.write_str("manual hotkey acknowledgement timed out"),
            Self::AckDisconnected => {
                formatter.write_str("manual hotkey acknowledgement disconnected")
            }
            Self::Runtime(error) => write!(formatter, "{error}"),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ManualWorkerSenders {
    #[cfg_attr(not(windows), allow(dead_code))]
    capture: Option<SyncSender<WorkerCommand>>,
    #[cfg_attr(not(windows), allow(dead_code))]
    recording: Option<SyncSender<WorkerCommand>>,
    #[cfg_attr(not(windows), allow(dead_code))]
    assets: Option<SyncSender<WorkerCommand>>,
    #[cfg_attr(not(windows), allow(dead_code))]
    hotkey: Option<SyncSender<WorkerCommand>>,
    pipe: Option<SyncSender<WorkerCommand>>,
}

impl ManualWorkerSenders {
    pub(crate) fn new(
        capture: Option<SyncSender<WorkerCommand>>,
        recording: Option<SyncSender<WorkerCommand>>,
        assets: Option<SyncSender<WorkerCommand>>,
        hotkey: Option<SyncSender<WorkerCommand>>,
        pipe: Option<SyncSender<WorkerCommand>>,
    ) -> Self {
        Self {
            capture,
            recording,
            assets,
            hotkey,
            pipe,
        }
    }

    #[cfg_attr(not(windows), allow(dead_code))]
    pub(super) fn dispatcher(
        &self,
        settings: crate::settings::ActiveSettings,
    ) -> ManualActionDispatcher {
        ManualActionDispatcher::new(
            settings,
            self.capture.clone(),
            self.recording.clone(),
            self.assets.clone(),
        )
    }

    pub(super) fn control_active_recording(
        &self,
        control: crate::protocol::RecordingControl,
    ) -> Result<(), ()> {
        let sender = self.recording.as_ref().ok_or(())?;
        sender
            .try_send(WorkerCommand::ControlActiveRecording { control })
            .map_err(|_| ())
    }

    #[cfg(any(windows, test))]
    pub(super) fn apply_manual_hotkeys(
        &self,
        hotkeys: &BTreeMap<String, String>,
    ) -> Result<(), ManualHotkeyApplyError> {
        if hotkeys.is_empty() {
            return Ok(());
        }
        let sender = self
            .hotkey
            .as_ref()
            .ok_or(ManualHotkeyApplyError::SenderUnavailable)?;
        let bindings = crate::hotkeys::bindings_from_manual_hotkeys(hotkeys)
            .map_err(|_| ManualHotkeyApplyError::InvalidBindings)?;
        let mut registry = crate::hotkeys::HotkeyRegistry::default();
        let plan = registry
            .apply_bindings(bindings)
            .map_err(|_| ManualHotkeyApplyError::InvalidBindings)?;
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        match sender.try_send(WorkerCommand::ApplyHotkeyPlan {
            plan,
            route: crate::hotkeys::HotkeyPlanRoute::Manual,
            result_sender,
        }) {
            Ok(()) => {}
            Err(mpsc::TrySendError::Full(_)) => {
                return Err(ManualHotkeyApplyError::CommandQueueFull);
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                return Err(ManualHotkeyApplyError::CommandQueueDisconnected);
            }
        }
        match result_receiver.recv_timeout(super::super::RUNTIME_APPLY_ACK_TIMEOUT) {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(ManualHotkeyApplyError::Runtime(error)),
            Err(mpsc::RecvTimeoutError::Timeout) => Err(ManualHotkeyApplyError::AckTimedOut),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err(ManualHotkeyApplyError::AckDisconnected)
            }
        }
    }

    #[cfg(any(windows, test))]
    pub(super) fn quiesce_extension_inputs(&self) -> Result<(), ()> {
        // Establish a clean media baseline first. A completed operation from earlier in the
        // process lifetime must not make a later mode switch fail.
        let _ = Self::fence(self.capture.as_ref())?;
        let _ = Self::fence(self.recording.as_ref())?;
        let _ = Self::fence(self.pipe.as_ref())?;
        let _ = Self::fence(self.hotkey.as_ref())?;
        // Anything admitted after the baseline while pipe ingress was closing is a real race and
        // must abort the transition.
        if Self::fence(self.capture.as_ref())? || Self::fence(self.recording.as_ref())? {
            return Err(());
        }
        Ok(())
    }

    #[cfg(any(windows, test))]
    pub(super) fn restore_hotkeys(&self) -> Result<(), ()> {
        let sender = self.hotkey.as_ref().ok_or(())?;
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        sender
            .try_send(WorkerCommand::RestoreHotkeys { result_sender })
            .map_err(|_| ())?;
        result_receiver
            .recv_timeout(super::super::RUNTIME_APPLY_ACK_TIMEOUT)
            .map_err(|_| ())?
            .map_err(|_| ())
    }

    #[cfg(any(windows, test))]
    fn fence(sender: Option<&SyncSender<WorkerCommand>>) -> Result<bool, ()> {
        let Some(sender) = sender else {
            return Ok(false);
        };
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        sender
            .try_send(WorkerCommand::FenceMode { result_sender })
            .map_err(|_| ())?;
        result_receiver
            .recv_timeout(super::super::RUNTIME_APPLY_ACK_TIMEOUT)
            .map_err(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, thread};

    use super::ManualWorkerSenders;
    use crate::app::resident::WorkerCommand;

    #[test]
    fn mode_fence_waits_for_each_runtime_port() -> Result<(), Box<dyn std::error::Error>> {
        let (capture_sender, capture_receiver) = mpsc::sync_channel(1);
        let (recording_sender, recording_receiver) = mpsc::sync_channel(1);
        let (hotkey_sender, hotkey_receiver) = mpsc::sync_channel(1);
        let (pipe_sender, pipe_receiver) = mpsc::sync_channel(1);
        let media_workers = [capture_receiver, recording_receiver].map(|receiver| {
            thread::spawn(move || {
                for _ in 0..2 {
                    if let Ok(WorkerCommand::FenceMode { result_sender }) = receiver.recv() {
                        let _ = result_sender.send(false);
                    }
                }
            })
        });
        let ingress_workers = [hotkey_receiver, pipe_receiver].map(|receiver| {
            thread::spawn(move || {
                if let Ok(WorkerCommand::FenceMode { result_sender }) = receiver.recv() {
                    let _ = result_sender.send(false);
                }
            })
        });
        let senders = ManualWorkerSenders::new(
            Some(capture_sender),
            Some(recording_sender),
            None,
            Some(hotkey_sender),
            Some(pipe_sender),
        );

        senders
            .quiesce_extension_inputs()
            .map_err(|()| std::io::Error::other("fence"))?;
        for worker in media_workers.into_iter().chain(ingress_workers) {
            worker.join().map_err(|_| std::io::Error::other("worker"))?;
        }
        Ok(())
    }

    #[test]
    fn mode_fence_ignores_media_work_completed_before_the_transition() -> Result<(), std::io::Error>
    {
        let (capture_sender, capture_receiver) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            for accepted in [true, false] {
                if let Ok(WorkerCommand::FenceMode { result_sender }) = capture_receiver.recv() {
                    let _ = result_sender.send(accepted);
                }
            }
        });
        let senders = ManualWorkerSenders::new(Some(capture_sender), None, None, None, None);

        assert_eq!(senders.quiesce_extension_inputs(), Ok(()));
        worker.join().map_err(|_| std::io::Error::other("worker"))?;
        Ok(())
    }

    #[test]
    fn mode_fence_detects_work_enqueued_while_pipe_ingress_closes() -> Result<(), std::io::Error> {
        let (capture_sender, capture_receiver) = mpsc::sync_channel(2);
        let (pipe_sender, pipe_receiver) = mpsc::sync_channel(1);
        let ingress_capture = capture_sender.clone();
        let pipe = thread::spawn(move || {
            if let Ok(WorkerCommand::FenceMode { result_sender }) = pipe_receiver.recv() {
                let _ = ingress_capture.send(WorkerCommand::RefreshTrayConnectionStatus);
                let _ = result_sender.send(false);
            }
        });
        let capture = thread::spawn(move || {
            let mut accepted = false;
            let mut fences = 0_u8;
            while let Ok(command) = capture_receiver.recv() {
                match command {
                    WorkerCommand::FenceMode { result_sender } => {
                        let _ = result_sender.send(accepted);
                        accepted = false;
                        fences = fences.saturating_add(1);
                        if fences == 2 {
                            break;
                        }
                    }
                    _ => accepted = true,
                }
            }
        });
        let senders =
            ManualWorkerSenders::new(Some(capture_sender), None, None, None, Some(pipe_sender));

        assert_eq!(senders.quiesce_extension_inputs(), Err(()));
        pipe.join().map_err(|_| std::io::Error::other("pipe"))?;
        capture
            .join()
            .map_err(|_| std::io::Error::other("capture"))?;
        Ok(())
    }

    #[test]
    fn rollback_waits_for_hotkey_projection_restore() -> Result<(), std::io::Error> {
        let (hotkey_sender, hotkey_receiver) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || {
            if let Ok(WorkerCommand::RestoreHotkeys { result_sender }) = hotkey_receiver.recv() {
                let _ = result_sender.send(Ok(()));
            }
        });
        let senders = ManualWorkerSenders::new(None, None, None, Some(hotkey_sender), None);

        assert_eq!(senders.restore_hotkeys(), Ok(()));
        worker.join().map_err(|_| std::io::Error::other("worker"))?;
        Ok(())
    }
}
