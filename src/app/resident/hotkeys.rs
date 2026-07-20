//! Hotkey worker command adapter for the resident agent coordinator.

#[path = "hotkeys/logging.rs"]
mod logging;
#[cfg(test)]
mod tests;

use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender},
    time::Duration,
};

use crate::hotkeys::{
    HotkeyEventSource, HotkeyPlanRoute, HotkeyRegistrar, HotkeyRuntime, HotkeyRuntimeError,
};

use super::{AgentComponent, AgentEventHub, RUNTIME_APPLY_ACK_TIMEOUT, WorkerCommand, WorkerEvent};
use crate::app::responses::{NativeHostSideEffectError, NativeHostSideEffects};
use logging::{log_hotkey_event, log_hotkey_publish_result, show_notice_if_not_queued};

const HOTKEY_EVENT_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub(super) fn run_hotkey_worker<R>(
    component: AgentComponent,
    event_sender: &SyncSender<WorkerEvent>,
    command_receiver: &Receiver<WorkerCommand>,
    registrar: &mut R,
    events: &AgentEventHub,
    tray_command_sender: Option<&SyncSender<WorkerCommand>>,
    log: Option<&super::AgentProcessLogSink>,
) where
    R: HotkeyRegistrar + HotkeyEventSource,
{
    let mut worker = HotkeyWorker::default();
    let _ = event_sender.send(WorkerEvent::Started(component));
    loop {
        match command_receiver.recv_timeout(HOTKEY_EVENT_POLL_INTERVAL) {
            Ok(command) => match worker.handle_command(registrar, command, log) {
                Ok(HotkeyWorkerControl::Continue) | Err(_) => {}
                Ok(HotkeyWorkerControl::Stop) => break,
            },
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        worker.drain_hotkey_events(registrar, events, tray_command_sender, log);
    }
}

#[derive(Debug)]
struct HotkeyWorker<Handle> {
    runtime: HotkeyRuntime<Handle>,
    route: HotkeyPlanRoute,
    fenced: Option<FencedHotkeys>,
}

#[derive(Debug)]
struct FencedHotkeys {
    registrations: Vec<crate::hotkeys::RegisteredHotkey>,
    route: HotkeyPlanRoute,
}

impl<Handle> Default for HotkeyWorker<Handle> {
    fn default() -> Self {
        Self {
            runtime: HotkeyRuntime::default(),
            route: HotkeyPlanRoute::Extension,
            fenced: None,
        }
    }
}

impl<Handle> HotkeyWorker<Handle> {
    fn handle_command<R>(
        &mut self,
        registrar: &mut R,
        command: WorkerCommand,
        log: Option<&super::AgentProcessLogSink>,
    ) -> Result<HotkeyWorkerControl, HotkeyRuntimeError>
    where
        R: HotkeyRegistrar<Handle = Handle>,
    {
        match command {
            WorkerCommand::FenceMode { result_sender } => {
                self.fenced = Some(FencedHotkeys {
                    registrations: self.runtime.active_registrations(),
                    route: self.route,
                });
                self.runtime.clear();
                self.route = HotkeyPlanRoute::Extension;
                let _ = result_sender.send(false);
                Ok(HotkeyWorkerControl::Continue)
            }
            WorkerCommand::RestoreHotkeys { result_sender } => {
                let result = self.restore_fenced(registrar);
                let _ = result_sender.send(result.clone());
                result?;
                Ok(HotkeyWorkerControl::Continue)
            }
            WorkerCommand::ApplyHotkeyPlan {
                plan,
                route,
                result_sender,
            } => {
                log_hotkey_event(
                    log,
                    "plan-apply-start",
                    "hotkey registration plan apply started",
                    None,
                );
                let result = self.runtime.apply_plan(registrar, &plan);
                if result.is_ok() {
                    self.route = route;
                }
                if let Err(error) = &result {
                    log_hotkey_event(
                        log,
                        "registration-failed",
                        &error.to_string(),
                        Some("hotkeys.registration_failed"),
                    );
                }
                let _ = result_sender.send(result.clone());
                result?;
                log_hotkey_event(
                    log,
                    "registration-applied",
                    "hotkey registration plan applied",
                    None,
                );
                Ok(HotkeyWorkerControl::Continue)
            }
            WorkerCommand::ApplyTrayRegistry { .. }
            | WorkerCommand::ManualHotkeyAction { .. }
            | WorkerCommand::ShowTrayNotice { .. }
            | WorkerCommand::UpdateTrayRecordingStatus { .. }
            | WorkerCommand::RefreshTrayConnectionStatus
            | WorkerCommand::CaptureScreenshot { .. }
            | WorkerCommand::StartRecording { .. }
            | WorkerCommand::ControlRecording { .. }
            | WorkerCommand::ControlActiveRecording { .. }
            | WorkerCommand::StartRecordingTelemetry { .. }
            | WorkerCommand::PauseRecordingTelemetry { .. }
            | WorkerCommand::ResumeRecordingTelemetry { .. }
            | WorkerCommand::FinishRecordingTelemetry { .. }
            | WorkerCommand::CancelRecordingTelemetry { .. }
            | WorkerCommand::DiscardRecordingTelemetry { .. }
            | WorkerCommand::SnapshotRecordingTelemetry { .. }
            | WorkerCommand::PrepareManualAsset { .. }
            | WorkerCommand::PublishManualAsset { .. } => Ok(HotkeyWorkerControl::Continue),
            #[cfg(test)]
            WorkerCommand::TrayMenuCommandSelected { .. } => Ok(HotkeyWorkerControl::Continue),
            WorkerCommand::Stop => Ok(HotkeyWorkerControl::Stop),
        }
    }

    fn restore_fenced<R>(&mut self, registrar: &mut R) -> Result<(), HotkeyRuntimeError>
    where
        R: HotkeyRegistrar<Handle = Handle>,
    {
        let Some(fenced) = self.fenced.as_ref() else {
            return Ok(());
        };
        self.runtime.clear();
        let plan = crate::hotkeys::HotkeyRegistrationPlan {
            unregister_ids: Vec::new(),
            register: fenced.registrations.clone(),
            restore_on_failure: Vec::new(),
        };
        self.runtime.apply_plan(registrar, &plan)?;
        self.route = fenced.route;
        self.fenced = None;
        Ok(())
    }

    #[cfg(test)]
    fn active_registration_ids(&self) -> Vec<u32> {
        self.runtime
            .active_registrations()
            .iter()
            .map(|registration| registration.registration_id)
            .collect()
    }

    #[cfg(test)]
    const fn active_route(&self) -> HotkeyPlanRoute {
        self.route
    }

    #[cfg(test)]
    fn active_action_ids(&self) -> Vec<String> {
        self.runtime
            .active_registrations()
            .into_iter()
            .map(|registration| registration.action_id)
            .collect()
    }

    #[cfg(test)]
    const fn has_fenced_snapshot(&self) -> bool {
        self.fenced.is_some()
    }

    fn drain_hotkey_events<S>(
        &self,
        source: &mut S,
        events: &AgentEventHub,
        tray_command_sender: Option<&SyncSender<WorkerCommand>>,
        log: Option<&super::AgentProcessLogSink>,
    ) where
        S: HotkeyEventSource,
    {
        for registration_id in source.drain_hotkey_events() {
            if let Some(action_id) = self.runtime.action_for_registration_id(registration_id) {
                log_hotkey_event(log, "event-received", "hotkey event received", None);
                match self.route {
                    HotkeyPlanRoute::Extension => {
                        let publish_result = events.publish_hotkey_action(action_id);
                        log_hotkey_publish_result(log, publish_result);
                        show_notice_if_not_queued(publish_result, tray_command_sender);
                    }
                    HotkeyPlanRoute::Manual => {
                        if let Some(sender) = tray_command_sender {
                            let _ = sender.send(WorkerCommand::ManualHotkeyAction {
                                action_id: action_id.to_owned(),
                            });
                        }
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum HotkeyWorkerControl {
    Continue,
    Stop,
}

pub(super) struct HotkeyWorkerPlanSink {
    sender: Option<SyncSender<WorkerCommand>>,
}

impl HotkeyWorkerPlanSink {
    pub(super) const fn new(sender: Option<SyncSender<WorkerCommand>>) -> Self {
        Self { sender }
    }
}

impl NativeHostSideEffects for HotkeyWorkerPlanSink {
    fn apply_hotkey_plan(
        &mut self,
        plan: crate::hotkeys::HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        let sender = self
            .sender
            .as_ref()
            .ok_or(NativeHostSideEffectError::HotkeyApply)?;
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        sender
            .try_send(WorkerCommand::ApplyHotkeyPlan {
                plan,
                route: HotkeyPlanRoute::Extension,
                result_sender,
            })
            .map_err(|_| NativeHostSideEffectError::HotkeyApply)?;
        result_receiver
            .recv_timeout(RUNTIME_APPLY_ACK_TIMEOUT)
            .map_err(|_| NativeHostSideEffectError::HotkeyApply)?
            .map_err(|_| NativeHostSideEffectError::HotkeyApply)
    }
}

#[cfg(not(windows))]
#[derive(Default)]
pub(super) struct NoopHotkeyRegistrar;

#[cfg(not(windows))]
pub(super) struct NoopHotkeyRegistration;

#[cfg(not(windows))]
impl HotkeyRegistrar for NoopHotkeyRegistrar {
    type Handle = NoopHotkeyRegistration;

    fn register(
        &mut self,
        _hotkey: &crate::hotkeys::RegisteredHotkey,
    ) -> Result<Self::Handle, HotkeyRuntimeError> {
        Ok(NoopHotkeyRegistration)
    }
}

#[cfg(not(windows))]
impl HotkeyEventSource for NoopHotkeyRegistrar {
    fn drain_hotkey_events(&mut self) -> Vec<u32> {
        Vec::new()
    }
}
