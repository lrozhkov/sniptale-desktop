//! Native-host session and resident shared state.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

#[cfg(not(test))]
use crate::app::responses::clock::system_epoch_ms;
use crate::{
    app::responses::{
        clock::SessionEpochSource, command::CommandResultCache, hotkeys::HotkeySettingsState,
    },
    ipc::{ControllerLeaseBook, ControllerLeaseDecision, ControllerLeaseError},
    protocol::{ControllerAcquireReason, NativeControllerIdentity},
    settings::SettingsSnapshotStore,
    telemetry::RecordingTelemetryPolicy,
    tray::TrayActionRegistry,
};

#[derive(Clone, Debug)]
pub(crate) struct NativeHostSharedState {
    lease_book: Arc<Mutex<ControllerLeaseBook>>,
    accepted_state: Arc<Mutex<NativeHostAcceptedState>>,
    tray_connection_generation: Arc<AtomicU64>,
}

impl Default for NativeHostSharedState {
    fn default() -> Self {
        Self {
            lease_book: Arc::new(Mutex::new(ControllerLeaseBook::default())),
            accepted_state: Arc::new(Mutex::new(NativeHostAcceptedState::default())),
            tray_connection_generation: Arc::new(AtomicU64::new(0)),
        }
    }
}

impl NativeHostSharedState {
    #[cfg(test)]
    pub(crate) fn poison_lease_book_for_tests(&self) {
        let lease_book = self.lease_book.clone();
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let Ok(_guard) = lease_book.lock() else {
                return;
            };
            std::panic::resume_unwind(Box::new("poison test lease book"));
        }));
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ActiveLeaseValidation {
    Valid,
    Invalid,
    StateUnavailable,
}

impl ActiveLeaseValidation {
    pub(super) const fn is_valid(self) -> bool {
        matches!(self, Self::Valid)
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct NativeHostAcceptedState {
    pub(super) settings_store: SettingsSnapshotStore,
    pub(super) hotkey_state: HotkeySettingsState,
    pub(super) tray_registry: Option<TrayActionRegistry>,
}

#[derive(Clone, Debug)]
pub(crate) struct NativeHostSession {
    pub(super) handshake_accepted: bool,
    pub(super) extension_id: Option<String>,
    pub(super) controller_lease_id: Option<String>,
    pub(super) active_controller: Option<NativeControllerIdentity>,
    tray_connection_generation: Option<u64>,
    shared_state: NativeHostSharedState,
    last_observed_epoch_ms: u64,
    pub(super) settings_store: SettingsSnapshotStore,
    pub(super) command_results: CommandResultCache,
    pub(super) hotkey_state: HotkeySettingsState,
    pub(super) tray_registry: Option<TrayActionRegistry>,
    epoch_source: SessionEpochSource,
}

impl Default for NativeHostSession {
    fn default() -> Self {
        Self {
            handshake_accepted: false,
            extension_id: None,
            controller_lease_id: None,
            active_controller: None,
            tray_connection_generation: None,
            shared_state: NativeHostSharedState::default(),
            last_observed_epoch_ms: 0,
            settings_store: SettingsSnapshotStore::default(),
            command_results: CommandResultCache::default(),
            hotkey_state: HotkeySettingsState::default(),
            tray_registry: None,
            epoch_source: SessionEpochSource::ObservedMessage,
        }
    }
}

impl NativeHostSession {
    #[cfg(test)]
    pub(crate) fn with_shared_state(shared_state: NativeHostSharedState) -> Self {
        Self {
            shared_state,
            ..Self::default()
        }
    }

    #[cfg(all(not(windows), not(test)))]
    pub(crate) fn system_clocked() -> Self {
        Self {
            epoch_source: SessionEpochSource::System,
            ..Self::default()
        }
    }

    #[cfg(not(test))]
    #[cfg(any(windows, test))]
    pub(crate) fn system_clocked_with_shared_state(shared_state: NativeHostSharedState) -> Self {
        Self {
            shared_state,
            epoch_source: SessionEpochSource::System,
            ..Self::default()
        }
    }

    #[cfg(any(windows, test))]
    pub(crate) fn controller_lease_id(&self) -> Option<&str> {
        self.controller_lease_id.as_deref()
    }

    pub(crate) fn begin_tray_connection_projection(&mut self) -> u64 {
        let generation = self
            .shared_state
            .tray_connection_generation
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        self.tray_connection_generation = Some(generation);
        generation
    }

    #[cfg(any(windows, test))]
    pub(crate) fn release_controller_lease(&mut self) -> Option<u64> {
        let Some(controller) = self.active_controller.take() else {
            self.controller_lease_id = None;
            self.tray_connection_generation = None;
            return None;
        };
        self.controller_lease_id = None;
        let generation = self.tray_connection_generation.take();
        let Ok(mut lease_book) = self.shared_state.lease_book.lock() else {
            return None;
        };
        lease_book
            .release(&controller)
            .then_some(generation)
            .flatten()
    }

    #[cfg(any(windows, test))]
    pub(crate) fn tray_registry(&self) -> Option<&TrayActionRegistry> {
        self.tray_registry.as_ref()
    }

    pub(super) fn settings_revision(&self) -> Option<&str> {
        self.settings_store
            .accepted()
            .map(|snapshot| snapshot.revision.as_str())
    }

    pub(super) fn max_recording_duration_ms(&self) -> Option<u64> {
        self.settings_store
            .accepted()
            .map(|snapshot| snapshot.max_recording_duration_ms)
    }

    pub(super) fn recording_telemetry_policy(&self) -> Option<RecordingTelemetryPolicy> {
        self.settings_store.accepted().map(|snapshot| {
            let telemetry = snapshot.telemetry;
            RecordingTelemetryPolicy {
                collect_cursor: telemetry.collect_cursor,
                collect_clicks: telemetry.collect_clicks,
                collect_key_events: telemetry.collect_key_events,
                collect_typing_spans: telemetry.collect_typing_spans,
                collect_static_signals: telemetry.collect_static_signals,
                telemetry_non_fatal: true,
            }
        })
    }

    pub(super) fn validate_active_lease(
        &self,
        controller_lease_id: &str,
        now_epoch_ms: u64,
    ) -> ActiveLeaseValidation {
        let Some(controller) = self.active_controller.as_ref() else {
            return ActiveLeaseValidation::Invalid;
        };
        let Ok(lease_book) = self.shared_state.lease_book.lock() else {
            return ActiveLeaseValidation::StateUnavailable;
        };
        if lease_book
            .validate(controller, controller_lease_id, now_epoch_ms)
            .is_ok()
        {
            ActiveLeaseValidation::Valid
        } else {
            ActiveLeaseValidation::Invalid
        }
    }

    pub(super) fn acquire_controller_lease(
        &mut self,
        controller: NativeControllerIdentity,
        reason: &ControllerAcquireReason,
        requested_at_epoch_ms: u64,
        requested_lease_id: String,
    ) -> Result<ControllerLeaseDecision, ControllerLeaseError> {
        self.shared_state
            .lease_book
            .lock()
            .map_err(|_| ControllerLeaseError::NoActiveLease)?
            .acquire(
                controller,
                reason,
                requested_at_epoch_ms,
                requested_lease_id,
            )
    }

    pub(super) fn update_shared_accepted_state<R>(
        &mut self,
        update: impl FnOnce(
            &mut SettingsSnapshotStore,
            &mut Option<TrayActionRegistry>,
            &mut HotkeySettingsState,
        ) -> R,
    ) -> Result<R, ()> {
        let mut accepted_state = self.shared_state.accepted_state.lock().map_err(|_| ())?;
        let NativeHostAcceptedState {
            settings_store,
            hotkey_state,
            tray_registry,
        } = &mut *accepted_state;
        let result = update(settings_store, tray_registry, hotkey_state);
        self.settings_store = accepted_state.settings_store.clone();
        self.tray_registry = accepted_state.tray_registry.clone();
        self.hotkey_state = accepted_state.hotkey_state.clone();
        Ok(result)
    }

    fn observe_epoch_ms(&mut self, epoch_ms: u64) {
        self.last_observed_epoch_ms = self.last_observed_epoch_ms.max(epoch_ms);
    }

    pub(super) fn trusted_epoch_ms(&mut self, observed_epoch_ms: u64) -> u64 {
        self.observe_epoch_ms(observed_epoch_ms);
        self.current_epoch_ms()
    }

    const fn observed_epoch_ms(&self) -> u64 {
        if self.last_observed_epoch_ms == 0 {
            1
        } else {
            self.last_observed_epoch_ms
        }
    }

    pub(super) fn current_epoch_ms(&self) -> u64 {
        match self.epoch_source {
            SessionEpochSource::ObservedMessage => self.observed_epoch_ms(),
            #[cfg(not(test))]
            SessionEpochSource::System => system_epoch_ms(),
        }
    }
}
