//! Tray command selection and current-state revalidation.

use std::sync::mpsc::SyncSender;

use crate::tray::{ShellActionId, TrayCommandSource, TrayConnectionStatus};

use super::{
    super::{AgentEventHub, AgentShutdownSignal},
    TrayWorker, dispatch,
};

impl TrayWorker {
    pub(super) fn dispatch_command(
        &mut self,
        command_id: u32,
        events: &AgentEventHub,
        shutdown_sender: &SyncSender<AgentShutdownSignal>,
        source: &mut impl TrayCommandSource,
    ) {
        let Some(command) = self
            .commands
            .iter()
            .find(|command| command.command_id == command_id)
        else {
            return;
        };
        if command.action_kind == crate::tray::TrayActionKind::Quit {
            match dispatch::quit::dispatch(self.icon_state.recording, &self.manual_senders, source)
            {
                dispatch::quit::QuitDisposition::ShutdownNow => {
                    let _ = shutdown_sender.try_send(AgentShutdownSignal::Stop);
                }
                dispatch::quit::QuitDisposition::WaitForRecordingIdle => {
                    self.exit_after_recording = true;
                }
                dispatch::quit::QuitDisposition::Continue => {}
            }
            return;
        }
        if command.action_kind == crate::tray::TrayActionKind::OpenSettings
            && command.action_id != ShellActionId::OpenExtensionSettings.as_str()
        {
            if command.action_id == ShellActionId::ModeExtensionManaged.as_str() {
                dispatch::open_extension_managed(
                    &self.active_settings,
                    &mut self.settings_window,
                    source,
                );
                return;
            }
            if command.action_id == ShellActionId::ModeManual.as_str() {
                dispatch::open_manual_setup(
                    &self.active_settings,
                    &mut self.settings_window,
                    source,
                );
                return;
            }
            dispatch::open_settings(&self.active_settings, &mut self.settings_window, source);
            return;
        }
        dispatch::dispatch_command(
            command,
            dispatch::TrayDispatchContext {
                active_settings: &self.active_settings,
                manual_dispatcher: self.manual_dispatcher.as_mut(),
                registry: self.registry.as_ref(),
                manual_senders: &self.manual_senders,
                connected: self.icon_state.connection == TrayConnectionStatus::Connected,
                events,
                shutdown_sender,
            },
            source,
        );
    }
}
