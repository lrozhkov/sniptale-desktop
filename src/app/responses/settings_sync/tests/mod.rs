use std::{error::Error, io::Error as IoError};

use serde_json::{Value, json};

use crate::{
    hotkeys::HotkeyRegistrationPlan,
    protocol::{
        ExtensionSettingsSyncMessage, SETTINGS_SCHEMA_VERSION, WireMessage, parse_wire_message,
    },
    settings::SettingsSnapshotStore,
    tray::TrayActionRegistry,
};

use super::super::{
    effects::{NativeHostSideEffectError, NativeHostSideEffects},
    responses_for_message_with_effects,
    session::{NativeHostSession, NativeHostSharedState},
};
use super::{HotkeySettingsState, settings_sync_responses};

#[test]
fn hotkey_runtime_failure_does_not_commit_new_settings_state() -> Result<(), Box<dyn Error>> {
    let mut settings_store = SettingsSnapshotStore::default();
    let mut tray_registry = None;
    let mut hotkey_state = HotkeySettingsState::default();
    let mut effects = RecordingEffects::default();
    let (initial_message, initial_value) = settings_sync("settings-revision-1", "Ctrl+Shift+S")?;

    let accepted = settings_sync_responses(
        true,
        &mut settings_store,
        &mut tray_registry,
        &mut hotkey_state,
        &initial_message,
        &initial_value,
        &mut effects,
    );

    assert_eq!(accepted[0]["type"], "app.settings.accepted");
    assert_eq!(
        accepted[0]["effectiveSettings"]["warnings"][0]["code"],
        "deferred-evaluation"
    );
    assert_eq!(
        accepted[0]["warnings"][0]["field"],
        "video.effectiveQuality"
    );
    assert_eq!(effects.tray_registries.len(), 1);
    assert_eq!(
        effects.tray_registries[0].actions()[0].id(),
        "screenshot-screen"
    );
    assert_eq!(
        settings_store
            .accepted()
            .ok_or_else(|| IoError::other("missing accepted settings"))?
            .revision,
        "settings-revision-1"
    );
    assert_eq!(
        settings_store
            .accepted()
            .ok_or_else(|| IoError::other("missing accepted settings"))?
            .max_recording_duration_ms,
        14_400_000
    );
    assert!(tray_registry.is_some());

    let mut failing_effects = FailingEffects;
    let (next_message, next_value) = settings_sync("settings-revision-2", "Ctrl+Shift+R")?;
    let rejected = settings_sync_responses(
        true,
        &mut settings_store,
        &mut tray_registry,
        &mut hotkey_state,
        &next_message,
        &next_value,
        &mut failing_effects,
    );

    assert_eq!(rejected[0]["type"], "app.operation.failed");
    assert_eq!(rejected[0]["phase"], "hotkeys-runtime");
    assert_eq!(rejected[0]["error"]["code"], "hotkey-runtime-failed");
    assert_eq!(
        settings_store
            .accepted()
            .ok_or_else(|| IoError::other("missing accepted settings"))?
            .revision,
        "settings-revision-1"
    );
    assert!(
        tray_registry
            .as_ref()
            .is_some_and(|registry: &TrayActionRegistry| !registry.actions().is_empty())
    );
    Ok(())
}

#[test]
fn tray_runtime_failure_rolls_back_hotkeys_and_does_not_commit_settings()
-> Result<(), Box<dyn Error>> {
    let mut settings_store = SettingsSnapshotStore::default();
    let mut tray_registry = None;
    let mut hotkey_state = HotkeySettingsState::default();
    let mut effects = RecordingEffects::default();
    let (initial_message, initial_value) = settings_sync("settings-revision-1", "Ctrl+Shift+S")?;

    let accepted = settings_sync_responses(
        true,
        &mut settings_store,
        &mut tray_registry,
        &mut hotkey_state,
        &initial_message,
        &initial_value,
        &mut effects,
    );

    assert_eq!(accepted[0]["type"], "app.settings.accepted");
    assert_eq!(
        settings_store
            .accepted()
            .ok_or_else(|| IoError::other("missing accepted settings"))?
            .revision,
        "settings-revision-1"
    );
    let previous_tray_registry = tray_registry
        .clone()
        .ok_or_else(|| IoError::other("missing accepted tray registry"))?;

    let mut tray_failure = TrayFailingEffects::default();
    let (next_message, next_value) = settings_sync("settings-revision-2", "Ctrl+Shift+R")?;
    let rejected = settings_sync_responses(
        true,
        &mut settings_store,
        &mut tray_registry,
        &mut hotkey_state,
        &next_message,
        &next_value,
        &mut tray_failure,
    );

    assert_eq!(rejected[0]["type"], "app.operation.failed");
    assert_eq!(rejected[0]["phase"], "tray-runtime");
    assert_eq!(rejected[0]["error"]["code"], "tray-runtime-failed");
    assert_eq!(
        settings_store
            .accepted()
            .ok_or_else(|| IoError::other("missing accepted settings"))?
            .revision,
        "settings-revision-1"
    );
    assert_eq!(tray_registry, Some(previous_tray_registry));
    assert_eq!(tray_failure.plans.len(), 2);
    assert_eq!(tray_failure.plans[0].unregister_ids, vec![1]);
    assert_eq!(tray_failure.plans[0].register[0].registration_id, 2);
    assert_eq!(tray_failure.plans[1].unregister_ids, vec![2]);
    assert_eq!(tray_failure.plans[1].register[0].registration_id, 1);
    Ok(())
}

#[test]
fn settings_sync_rejects_unsupported_schema_version_without_commit() -> Result<(), Box<dyn Error>> {
    let mut settings_store = SettingsSnapshotStore::default();
    let mut tray_registry = None;
    let mut hotkey_state = HotkeySettingsState::default();
    let mut effects = RecordingEffects::default();
    let (mut message, value) = settings_sync("settings-revision-unsupported", "Ctrl+Shift+S")?;
    message.schema_version = SETTINGS_SCHEMA_VERSION + 1;

    let responses = settings_sync_responses(
        true,
        &mut settings_store,
        &mut tray_registry,
        &mut hotkey_state,
        &message,
        &value,
        &mut effects,
    );

    assert_eq!(responses[0]["type"], "app.operation.failed");
    assert_eq!(responses[0]["phase"], "schema-version");
    assert_eq!(responses[0]["error"]["code"], "incompatible-settings");
    assert!(settings_store.accepted().is_none());
    assert!(tray_registry.is_none());
    assert!(effects.tray_registries.is_empty());
    Ok(())
}

#[test]
fn settings_hotkey_state_is_shared_across_resident_pipe_sessions() -> Result<(), Box<dyn Error>> {
    let shared_state = NativeHostSharedState::default();
    let mut first = NativeHostSession::with_shared_state(shared_state.clone());
    let mut first_effects = RecordingEffects::default();
    connect_and_sync_settings(&mut first, &mut first_effects, "settings-revision-1")?;
    assert_eq!(first_effects.plans.len(), 1);
    assert_eq!(first_effects.plans[0].register.len(), 1);
    let _ = first.release_controller_lease();

    let mut second = NativeHostSession::with_shared_state(shared_state);
    let mut second_effects = RejectDuplicateRegistrationEffects;
    let responses =
        connect_and_sync_settings(&mut second, &mut second_effects, "settings-revision-1")?;

    assert_eq!(responses[0]["type"], "app.settings.accepted");
    Ok(())
}

fn settings_sync(
    revision: &str,
    shortcut_label: &str,
) -> Result<(ExtensionSettingsSyncMessage, Value), Box<dyn Error>> {
    let value = json!({
        "type": "extension.settings.sync",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "revision": revision,
        "schemaVersion": 1,
        "generatedAtEpochMs": 2000,
        "settings": {
            "screenshots": {
                "includeCursor": true
            },
            "video": {
                "enabled": true,
                "advanced": {
                    "audioBitrateKbps": 160,
                    "audioSourceMode": "mixed",
                    "frameRate": 30,
                    "includeCursorInVideo": true,
                    "maxDurationMinutes": 240,
                    "preferHardwareEncoder": true,
                    "videoBitrateMbpsOverride": null
                },
                "codec": {
                    "audioCodec": "aac",
                    "container": "mp4",
                    "hardwareAcceleration": "prefer",
                    "videoCodec": "h264"
                },
                "telemetry": {
                    "collectCursor": true,
                    "collectClicks": true,
                    "collectKeyEvents": true,
                    "collectTypingSpans": true,
                    "collectStaticSignals": true
                }
            },
            "trayActions": {
                "actions": [{
                    "enabled": true,
                    "id": "screenshot-screen",
                    "kind": "capture-screenshot",
                    "label": "Capture",
                    "shortcutLabel": shortcut_label,
                    "offlineCapable": false
                }],
                "revision": revision
            }
        }
    });
    let payload = serde_json::to_vec(&value)?;
    let WireMessage::ExtensionSettingsSync(message) = parse_wire_message(&payload)? else {
        return Err(IoError::other("settings sync parsed as unexpected message").into());
    };
    Ok((*message, value))
}

fn connect_and_sync_settings(
    session: &mut NativeHostSession,
    effects: &mut impl NativeHostSideEffects,
    revision: &str,
) -> Result<Vec<Value>, Box<dyn Error>> {
    for payload in [extension_hello(), extension_controller_acquire()] {
        let message = parse_wire_message(payload)?;
        let value: Value = serde_json::from_slice(payload)?;
        responses_for_message_with_effects(session, &message, &value, effects);
    }
    let (settings, value) = settings_sync(revision, "Ctrl+Shift+S")?;
    Ok(responses_for_message_with_effects(
        session,
        &WireMessage::ExtensionSettingsSync(Box::new(settings)),
        &value,
        effects,
    ))
}

fn extension_hello() -> &'static [u8] {
    br#"{
        "type": "extension.hello",
        "protocolVersion": 1,
        "settingsSchemaVersion": 1,
        "extensionVersion": "0.1.0",
        "extensionId": "abcdefghijklmnopqrstuvwxzyabcdef",
        "minAppVersion": "0.1.0",
        "supportedProtocolVersions": [1],
        "supportedSettingsSchemaVersions": [1]
    }"#
}

fn extension_controller_acquire() -> &'static [u8] {
    br#"{
        "type": "extension.controller.acquire",
        "protocolVersion": 1,
        "connectionId": "connection-1",
        "browserFamily": "chrome",
        "profileKey": "profile-1",
        "documentId": "document-1",
        "requestedAtEpochMs": 1000,
        "reason": "initial-connect"
    }"#
}

#[derive(Default)]
struct RecordingEffects {
    plans: Vec<HotkeyRegistrationPlan>,
    tray_registries: Vec<TrayActionRegistry>,
}

impl NativeHostSideEffects for RecordingEffects {
    fn apply_hotkey_plan(
        &mut self,
        plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        self.plans.push(plan);
        Ok(())
    }

    fn apply_tray_registry(
        &mut self,
        registry: TrayActionRegistry,
    ) -> Result<(), NativeHostSideEffectError> {
        self.tray_registries.push(registry);
        Ok(())
    }
}

struct FailingEffects;

impl NativeHostSideEffects for FailingEffects {
    fn apply_hotkey_plan(
        &mut self,
        _plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        Err(NativeHostSideEffectError::HotkeyApply)
    }
}

#[derive(Default)]
struct TrayFailingEffects {
    plans: Vec<HotkeyRegistrationPlan>,
}

impl NativeHostSideEffects for TrayFailingEffects {
    fn apply_hotkey_plan(
        &mut self,
        plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        self.plans.push(plan);
        Ok(())
    }

    fn apply_tray_registry(
        &mut self,
        _registry: TrayActionRegistry,
    ) -> Result<(), NativeHostSideEffectError> {
        Err(NativeHostSideEffectError::TrayApply)
    }
}

struct RejectDuplicateRegistrationEffects;

impl NativeHostSideEffects for RejectDuplicateRegistrationEffects {
    fn apply_hotkey_plan(
        &mut self,
        plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        if plan.register.is_empty() {
            Ok(())
        } else {
            Err(NativeHostSideEffectError::HotkeyApply)
        }
    }
}
