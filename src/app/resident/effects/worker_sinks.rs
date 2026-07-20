//! Settings-derived tray and hotkey worker side effects.

use super::agent::AgentSideEffects;
use crate::{
    app::responses::{NativeHostSideEffectError, NativeHostSideEffects},
    hotkeys::HotkeyRegistrationPlan,
    tray::{TrayActionRegistry, TrayNoticeKind},
};

impl AgentSideEffects {
    pub(super) fn apply_hotkey_plan_to_worker(
        &mut self,
        plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        self.hotkeys.apply_hotkey_plan(plan)
    }

    pub(super) fn apply_tray_registry_to_worker(
        &mut self,
        registry: TrayActionRegistry,
    ) -> Result<(), NativeHostSideEffectError> {
        self.tray.apply_tray_registry(registry)
    }

    pub(super) fn show_tray_notice_in_worker(
        &mut self,
        kind: TrayNoticeKind,
    ) -> Result<(), NativeHostSideEffectError> {
        self.tray.show_tray_notice(kind)
    }

    pub(super) fn mark_tray_connection_connected_in_worker(
        &mut self,
        generation: u64,
    ) -> Result<(), NativeHostSideEffectError> {
        self.tray.mark_tray_connection_connected(generation)
    }

    pub(super) fn mark_tray_connection_disconnected_in_worker(
        &mut self,
        generation: u64,
    ) -> Result<(), NativeHostSideEffectError> {
        self.tray.mark_tray_connection_disconnected(generation)
    }
}
