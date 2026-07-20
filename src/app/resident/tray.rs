//! Tray worker state derived from accepted extension settings.

mod commands;
mod connection;
mod dispatch;
mod manual_senders;
mod recording;
mod runner;
mod sink;
#[cfg(test)]
mod tests;
mod view;

#[cfg(any(windows, test))]
use crate::tray::TrayRuntimeEvent;
use crate::tray::{
    TrayActionRegistry, TrayCommandSource, TrayConnectionStatus, TrayIconState,
    TrayRecordingStatus, build_manual_menu, build_menu, manual_tray_icon_tooltip, plan_menu_render,
    tray_icon_tooltip, tray_icon_variant,
};
use std::sync::mpsc::SyncSender;

use super::control::manual_dispatch::ManualActionDispatcher;
use super::{AgentEventHub, AgentShutdownSignal, AgentStartupStatus, WorkerCommand};
pub(super) use connection::TrayConnectionState;
pub(super) use manual_senders::{ManualHotkeyApplyError, ManualWorkerSenders};
pub(super) use recording::TrayRecordingState;
pub(super) use runner::{TrayWorkerInputs, run_tray_worker};
pub(super) use sink::TrayWorkerRegistrySink;
use view::{TrayWorkerControl, tray_startup_status};

#[derive(Debug)]
struct TrayWorker {
    icon_state: TrayIconState,
    registry: Option<TrayActionRegistry>,
    commands: Vec<crate::tray::TrayMenuCommand>,
    manual_dispatcher: Option<ManualActionDispatcher>,
    #[cfg_attr(not(windows), allow(dead_code))]
    manual_senders: ManualWorkerSenders,
    active_settings: crate::settings::ActiveSettings,
    settings_window: crate::tray::settings_window::model::SettingsWindowModel,
    connection_state: TrayConnectionState,
    recording_state: TrayRecordingState,
    exit_after_recording: bool,
}
impl TrayWorker {
    #[cfg(test)]
    fn new(
        startup_status: AgentStartupStatus,
        active_settings: crate::settings::ActiveSettings,
        manual_dispatcher: Option<ManualActionDispatcher>,
        manual_senders: ManualWorkerSenders,
    ) -> Self {
        Self::with_projection_states(
            startup_status,
            active_settings,
            manual_dispatcher,
            manual_senders,
            TrayConnectionState::default(),
            TrayRecordingState::default(),
        )
    }

    fn with_projection_states(
        startup_status: AgentStartupStatus,
        active_settings: crate::settings::ActiveSettings,
        manual_dispatcher: Option<ManualActionDispatcher>,
        manual_senders: ManualWorkerSenders,
        connection_state: TrayConnectionState,
        recording_state: TrayRecordingState,
    ) -> Self {
        Self {
            icon_state: TrayIconState {
                connection: TrayConnectionStatus::Disconnected,
                recording: TrayRecordingStatus::Idle,
                startup_status: tray_startup_status(startup_status),
            },
            registry: None,
            commands: Vec::new(),
            manual_dispatcher,
            manual_senders,
            active_settings,
            settings_window: crate::tray::settings_window::model::SettingsWindowModel::default(),
            connection_state,
            recording_state,
            exit_after_recording: false,
        }
    }
    fn render_fallback_menu(
        &mut self,
        source: &mut impl TrayCommandSource,
    ) -> Result<(), crate::tray::TrayRuntimeError> {
        let menu = if self.manual_dispatcher.is_some() {
            build_manual_menu(self.icon_state.recording)
        } else {
            build_menu(None, false, self.icon_state.recording)
        };
        let plan =
            plan_menu_render(&menu).map_err(|_| crate::tray::TrayRuntimeError::RenderFailed)?;
        source.apply_render_plan(&plan, &self.tooltip(), tray_icon_variant(&self.icon_state))?;
        self.commands = plan.commands;
        self.registry = None;
        Ok(())
    }

    fn handle_command(
        &mut self,
        command: WorkerCommand,
        events: &AgentEventHub,
        shutdown_sender: &SyncSender<AgentShutdownSignal>,
        source: &mut impl TrayCommandSource,
    ) -> TrayWorkerControl {
        #[cfg(not(test))]
        let _ = (events, shutdown_sender);
        match command {
            WorkerCommand::ApplyTrayRegistry {
                registry,
                result_sender,
            } => {
                let result = self.apply_registry(registry, source);
                let _ = result_sender.send(result);
                TrayWorkerControl::Continue
            }
            WorkerCommand::ShowTrayNotice { kind } => {
                let _ = source.show_notice(kind);
                TrayWorkerControl::Continue
            }
            WorkerCommand::FenceMode { result_sender } => {
                let _ = result_sender.send(false);
                TrayWorkerControl::Continue
            }
            WorkerCommand::UpdateTrayRecordingStatus { status } => {
                self.apply_recording_status(status, shutdown_sender, source);
                TrayWorkerControl::Continue
            }
            WorkerCommand::RefreshTrayConnectionStatus => {
                self.sync_connection_status(source);
                TrayWorkerControl::Continue
            }
            WorkerCommand::ManualHotkeyAction { action_id } => {
                self.dispatch_manual_hotkey(&action_id, source);
                TrayWorkerControl::Continue
            }
            #[cfg(test)]
            WorkerCommand::TrayMenuCommandSelected { command_id } => {
                self.dispatch_command(command_id, events, shutdown_sender, source);
                TrayWorkerControl::Continue
            }
            WorkerCommand::ApplyHotkeyPlan { .. }
            | WorkerCommand::RestoreHotkeys { .. }
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
            | WorkerCommand::PublishManualAsset { .. } => TrayWorkerControl::Continue,
            WorkerCommand::Stop => TrayWorkerControl::Stop,
        }
    }

    fn dispatch_manual_hotkey(&mut self, action_id: &str, source: &mut impl TrayCommandSource) {
        let Some(dispatcher) = self.manual_dispatcher.as_mut() else {
            let _ = source.show_notice(crate::tray::TrayNoticeKind::ActionRejected);
            return;
        };
        if action_id == crate::settings::ManualHotkeyAction::RecordingPauseResume.as_str() {
            let control = match self.icon_state.recording {
                TrayRecordingStatus::Recording => Some(crate::protocol::RecordingControl::Pause),
                TrayRecordingStatus::Paused => Some(crate::protocol::RecordingControl::Resume),
                _ => None,
            };
            if control.is_none_or(|control| dispatcher.control_recording(&control).is_err()) {
                let _ = source.show_notice(crate::tray::TrayNoticeKind::ActionRejected);
            }
            return;
        }
        let _ = dispatch::dispatch_manual_command(action_id, dispatcher, source);
    }

    fn apply_registry(
        &mut self,
        registry: TrayActionRegistry,
        source: &mut impl TrayCommandSource,
    ) -> Result<(), crate::tray::TrayRuntimeError> {
        if self.manual_dispatcher.is_some() {
            return Ok(());
        }
        let menu = build_menu(Some(&registry), true, self.icon_state.recording);
        let plan =
            plan_menu_render(&menu).map_err(|_| crate::tray::TrayRuntimeError::RenderFailed)?;
        self.icon_state.connection = TrayConnectionStatus::Connected;
        source.apply_render_plan(&plan, &self.tooltip(), tray_icon_variant(&self.icon_state))?;
        self.commands = plan.commands;
        self.registry = Some(registry);
        Ok(())
    }

    fn sync_connection_status(&mut self, source: &mut impl TrayCommandSource) {
        let status = self.connection_state.status();
        if self.icon_state.connection != status {
            self.icon_state.connection = status;
            self.render_current_menu(source);
        }
    }

    fn render_current_menu(&mut self, source: &mut impl TrayCommandSource) {
        let menu = if self.manual_dispatcher.is_some() {
            build_manual_menu(self.icon_state.recording)
        } else {
            build_menu(
                self.registry.as_ref(),
                self.icon_state.connection == TrayConnectionStatus::Connected,
                self.icon_state.recording,
            )
        };
        if let Ok(plan) = plan_menu_render(&menu)
            && source
                .apply_render_plan(&plan, &self.tooltip(), tray_icon_variant(&self.icon_state))
                .is_ok()
        {
            self.commands = plan.commands;
        }
    }

    fn tooltip(&self) -> String {
        if self.manual_dispatcher.is_some() {
            manual_tray_icon_tooltip(self.icon_state.recording)
        } else {
            tray_icon_tooltip(&self.icon_state)
        }
    }

    fn drain_source_events(source: &mut impl TrayCommandSource) -> Vec<TrayRuntimeEvent> {
        source.drain_events().unwrap_or_default()
    }

    fn dispatch_source_events(
        &mut self,
        source_events: Vec<TrayRuntimeEvent>,
        events: &AgentEventHub,
        shutdown_sender: &SyncSender<AgentShutdownSignal>,
        source: &mut impl TrayCommandSource,
    ) {
        #[cfg(any(windows, test))]
        {
            for event in source_events {
                match event {
                    TrayRuntimeEvent::MenuCommand { command_id } => {
                        self.dispatch_command(command_id, events, shutdown_sender, source);
                    }
                    #[cfg(any(windows, test))]
                    TrayRuntimeEvent::SettingsWindow { event } => {
                        if dispatch::dispatch_settings_event(
                            event,
                            &self.active_settings,
                            &mut self.settings_window,
                            &mut self.manual_dispatcher,
                            &self.manual_senders,
                            self.icon_state.recording,
                            source,
                        ) {
                            self.icon_state.connection = TrayConnectionStatus::Disconnected;
                            self.connection_state.clear();
                            self.registry = None;
                            self.render_current_menu(source);
                        }
                    }
                }
            }
        }
        #[cfg(not(any(windows, test)))]
        {
            let _ = self.commands.len();
            let _ = (events, shutdown_sender, source_events);
        }
    }
}
