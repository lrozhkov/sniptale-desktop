//! Fallible tray settings apply sink for settings-sync side effects.

use std::sync::mpsc::{self, SyncSender};

use crate::{
    app::responses::{NativeHostSideEffectError, NativeHostSideEffects},
    tray::{TrayActionRegistry, TrayNoticeKind},
};

use super::super::{RUNTIME_APPLY_ACK_TIMEOUT, WorkerCommand};

pub(in crate::app::resident) struct TrayWorkerRegistrySink {
    sender: Option<SyncSender<WorkerCommand>>,
    connection_state: super::TrayConnectionState,
}

impl TrayWorkerRegistrySink {
    #[cfg(test)]
    pub(in crate::app::resident) fn new(sender: Option<SyncSender<WorkerCommand>>) -> Self {
        Self::with_connection_state(sender, super::TrayConnectionState::default())
    }

    pub(in crate::app::resident) fn with_connection_state(
        sender: Option<SyncSender<WorkerCommand>>,
        connection_state: super::TrayConnectionState,
    ) -> Self {
        Self {
            sender,
            connection_state,
        }
    }

    fn submit_tray_registry(
        &mut self,
        registry: TrayActionRegistry,
    ) -> Result<(), NativeHostSideEffectError> {
        let Some(sender) = self.sender.as_ref() else {
            return Err(NativeHostSideEffectError::TrayApply);
        };
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        if sender
            .try_send(WorkerCommand::ApplyTrayRegistry {
                registry,
                result_sender,
            })
            .is_err()
        {
            return Err(NativeHostSideEffectError::TrayApply);
        }
        if matches!(
            result_receiver.recv_timeout(RUNTIME_APPLY_ACK_TIMEOUT),
            Ok(Ok(()))
        ) {
            return Ok(());
        }
        Err(NativeHostSideEffectError::TrayApply)
    }

    fn submit_notice(&mut self, kind: TrayNoticeKind) {
        let Some(sender) = self.sender.as_ref() else {
            return;
        };
        let _ = sender.try_send(WorkerCommand::ShowTrayNotice { kind });
    }

    fn refresh_connection_projection(&self) {
        if let Some(sender) = self.sender.as_ref() {
            let _ = sender.try_send(WorkerCommand::RefreshTrayConnectionStatus);
        }
    }
}

impl NativeHostSideEffects for TrayWorkerRegistrySink {
    fn apply_hotkey_plan(
        &mut self,
        _plan: crate::hotkeys::HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        Ok(())
    }

    fn apply_tray_registry(
        &mut self,
        registry: TrayActionRegistry,
    ) -> Result<(), NativeHostSideEffectError> {
        self.submit_tray_registry(registry)
    }

    fn show_tray_notice(&mut self, kind: TrayNoticeKind) -> Result<(), NativeHostSideEffectError> {
        self.submit_notice(kind);
        Ok(())
    }

    fn mark_tray_connection_connected(
        &mut self,
        generation: u64,
    ) -> Result<(), NativeHostSideEffectError> {
        self.connection_state.set_connected(generation);
        self.refresh_connection_projection();
        Ok(())
    }

    fn mark_tray_connection_disconnected(
        &mut self,
        generation: u64,
    ) -> Result<(), NativeHostSideEffectError> {
        if self.connection_state.disconnect_if_current(generation) {
            self.refresh_connection_projection();
        }
        Ok(())
    }
}
