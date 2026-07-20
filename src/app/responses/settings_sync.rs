//! Native-host settings sync acceptance policy.

#[cfg(test)]
mod tests;

use serde_json::Value;

use crate::{
    hotkeys::bindings_from_action_catalog,
    protocol::{
        ExtensionSettingsSyncMessage, OperationFailedOperation, OperationalErrorKind,
        SETTINGS_SCHEMA_VERSION,
    },
    settings::{
        AcceptedRecordingTelemetrySettings, AcceptedSettingsSnapshot, SettingsSnapshotStore,
        action_catalog_from_native_actions, max_recording_duration_ms_from_minutes,
    },
    tray::{TrayActionRegistry, registry_from_action_catalog},
};

use super::NativeHostSession;
use super::effects::NativeHostSideEffects;
use super::hotkeys::{HotkeySettingsState, HotkeySettingsUpdate};
use super::outbound::{OperationFailure, operation_failed, settings_accepted};
use super::session::ActiveLeaseValidation;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SettingsSyncFailureReason {
    kind: OperationalErrorKind,
    phase: &'static str,
    message: &'static str,
}

impl SettingsSyncFailureReason {
    const fn new(kind: OperationalErrorKind, phase: &'static str, message: &'static str) -> Self {
        Self {
            kind,
            phase,
            message,
        }
    }
}

pub(super) fn settings_sync_message_responses(
    session: &mut NativeHostSession,
    settings: &ExtensionSettingsSyncMessage,
    value: &Value,
    effects: &mut impl NativeHostSideEffects,
) -> Vec<Value> {
    let now_epoch_ms = session.trusted_epoch_ms(settings.generated_at_epoch_ms);
    let lease_validation =
        session.validate_active_lease(&settings.controller_lease_id, now_epoch_ms);
    if lease_validation == ActiveLeaseValidation::StateUnavailable {
        return vec![settings_sync_failure(
            settings,
            OperationalErrorKind::SettingsStateUnavailable,
            "lease-validation",
            "controller lease state is unavailable",
        )];
    }
    match session.update_shared_accepted_state(|settings_store, tray_registry, hotkey_state| {
        settings_sync_responses(
            lease_validation.is_valid(),
            settings_store,
            tray_registry,
            hotkey_state,
            settings,
            value,
            effects,
        )
    }) {
        Ok(responses) => responses,
        Err(()) => vec![settings_sync_failure(
            settings,
            OperationalErrorKind::SettingsStateUnavailable,
            "state-lock",
            "settings state is unavailable",
        )],
    }
}

pub(super) fn settings_sync_responses(
    lease_valid: bool,
    settings_store: &mut SettingsSnapshotStore,
    tray_registry: &mut Option<TrayActionRegistry>,
    hotkey_state: &mut HotkeySettingsState,
    settings: &ExtensionSettingsSyncMessage,
    value: &Value,
    effects: &mut impl NativeHostSideEffects,
) -> Vec<Value> {
    if !lease_valid {
        return vec![stale_lease_failure(settings)];
    }
    if let Err(reason) = validate_settings_schema(settings) {
        return vec![settings_sync_failure(
            settings,
            reason.kind,
            reason.phase,
            reason.message,
        )];
    }
    let action_catalog = match action_catalog_from_native_actions(&settings.settings.tray_actions) {
        Ok(catalog) => catalog,
        Err(reason) => {
            let failure = action_catalog_failure(reason);
            return vec![settings_sync_failure(
                settings,
                failure.kind,
                failure.phase,
                failure.message,
            )];
        }
    };
    let accepted_tray_registry = registry_from_action_catalog(&action_catalog);
    let Ok(accepted_hotkey_bindings) = bindings_from_action_catalog(&action_catalog) else {
        return vec![settings_sync_failure(
            settings,
            OperationalErrorKind::HotkeyBindingsInvalid,
            "hotkeys",
            "hotkey bindings are invalid",
        )];
    };
    let Ok(hotkey_update) = hotkey_state.prepare(accepted_hotkey_bindings) else {
        return vec![settings_sync_failure(
            settings,
            OperationalErrorKind::HotkeyBindingsInvalid,
            "hotkeys",
            "hotkey bindings are invalid",
        )];
    };
    let snapshot = match accepted_settings_snapshot(settings, value) {
        Ok(snapshot) => snapshot,
        Err(reason) => {
            return vec![settings_sync_failure(
                settings,
                reason.kind,
                reason.phase,
                reason.message,
            )];
        }
    };
    let mut settings_store_update = settings_store.clone();
    if settings_store_update.accept(snapshot).is_err() {
        return vec![settings_sync_failure(
            settings,
            OperationalErrorKind::SettingsSnapshotInvalid,
            "snapshot",
            "settings snapshot is invalid",
        )];
    }
    if let Err(reason) = apply_runtime_settings(effects, &hotkey_update, &accepted_tray_registry) {
        return vec![settings_sync_failure(
            settings,
            reason.kind,
            reason.phase,
            reason.message,
        )];
    }
    *settings_store = settings_store_update;
    *tray_registry = Some(accepted_tray_registry);
    hotkey_state.commit(hotkey_update);
    vec![settings_accepted(settings, settings.generated_at_epoch_ms)]
}

fn validate_settings_schema(
    settings: &ExtensionSettingsSyncMessage,
) -> Result<(), SettingsSyncFailureReason> {
    if settings.schema_version == SETTINGS_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(SettingsSyncFailureReason::new(
            OperationalErrorKind::SettingsSchemaVersionIncompatible,
            "schema-version",
            "settings schema version is incompatible",
        ))
    }
}

const fn action_catalog_failure(
    _error: crate::settings::ActionCatalogError,
) -> SettingsSyncFailureReason {
    SettingsSyncFailureReason::new(
        OperationalErrorKind::TrayRegistryInvalid,
        "tray-actions",
        "tray action registry is invalid",
    )
}

fn accepted_settings_snapshot(
    settings: &ExtensionSettingsSyncMessage,
    value: &Value,
) -> Result<AcceptedSettingsSnapshot, SettingsSyncFailureReason> {
    Ok(AcceptedSettingsSnapshot {
        controller_lease_id: settings.controller_lease_id.clone(),
        revision: settings.revision.clone(),
        schema_version: settings.schema_version,
        generated_at_epoch_ms: settings.generated_at_epoch_ms,
        max_recording_duration_ms: max_recording_duration_ms(settings)?,
        telemetry: AcceptedRecordingTelemetrySettings {
            collect_cursor: settings.settings.video.telemetry.collect_cursor,
            collect_clicks: settings.settings.video.telemetry.collect_clicks,
            collect_key_events: settings.settings.video.telemetry.collect_key_events,
            collect_typing_spans: settings.settings.video.telemetry.collect_typing_spans,
            collect_static_signals: settings.settings.video.telemetry.collect_static_signals,
        },
        settings: value["settings"].clone(),
    })
}

fn max_recording_duration_ms(
    settings: &ExtensionSettingsSyncMessage,
) -> Result<u64, SettingsSyncFailureReason> {
    match settings
        .settings
        .video
        .advanced
        .max_duration_minutes
        .as_f64()
        .map(max_recording_duration_ms_from_minutes)
    {
        Some(Ok(duration_ms)) => Ok(duration_ms),
        Some(Err(_)) | None => Err(SettingsSyncFailureReason::new(
            OperationalErrorKind::SettingsSnapshotInvalid,
            "video.maxDurationMinutes",
            "max recording duration is invalid",
        )),
    }
}

fn apply_runtime_settings(
    effects: &mut impl NativeHostSideEffects,
    hotkey_update: &HotkeySettingsUpdate,
    accepted_tray_registry: &TrayActionRegistry,
) -> Result<(), SettingsSyncFailureReason> {
    if effects
        .apply_hotkey_plan(hotkey_update.registration_plan().clone())
        .is_err()
    {
        return Err(SettingsSyncFailureReason::new(
            OperationalErrorKind::HotkeyRuntimeFailed,
            "hotkeys-runtime",
            "hotkey registration plan could not be applied",
        ));
    }
    if effects
        .apply_tray_registry(accepted_tray_registry.clone())
        .is_err()
    {
        if effects
            .apply_hotkey_plan(hotkey_update.rollback_plan())
            .is_err()
        {
            return Err(SettingsSyncFailureReason::new(
                OperationalErrorKind::HotkeyRuntimeFailed,
                "hotkeys-runtime",
                "hotkey registration rollback could not be applied",
            ));
        }
        return Err(SettingsSyncFailureReason::new(
            OperationalErrorKind::TrayRuntimeFailed,
            "tray-runtime",
            "tray action registry could not be applied",
        ));
    }
    Ok(())
}

fn stale_lease_failure(settings: &ExtensionSettingsSyncMessage) -> Value {
    operation_failed(OperationFailure {
        controller_lease_id: Some(&settings.controller_lease_id),
        operation: OperationFailedOperation::SettingsSync,
        operation_id: Some(&settings.revision),
        phase: "lease-validation",
        kind: OperationalErrorKind::StaleControllerLease,
        message: None,
        occurred_at_epoch_ms: settings.generated_at_epoch_ms,
    })
}

fn settings_sync_failure(
    settings: &ExtensionSettingsSyncMessage,
    kind: OperationalErrorKind,
    phase: &str,
    message: &str,
) -> Value {
    operation_failed(OperationFailure {
        controller_lease_id: Some(&settings.controller_lease_id),
        operation: OperationFailedOperation::SettingsSync,
        operation_id: Some(&settings.revision),
        phase,
        kind,
        message: Some(message),
        occurred_at_epoch_ms: settings.generated_at_epoch_ms,
    })
}
