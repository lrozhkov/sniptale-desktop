//! Browser status projection from the resident's local settings authority.

use super::NativeHostSession;
use crate::{
    protocol::{ExtensionSettingsSyncMessage, PROTOCOL_VERSION, SETTINGS_SCHEMA_VERSION},
    settings::desktop_settings_snapshot,
};
use serde_json::Value;

pub(super) fn reconcile_selection(
    session: &mut NativeHostSession,
    effects: &mut impl super::NativeHostSideEffects,
) -> Vec<Value> {
    let now_ms = session.current_epoch_ms();
    let Some(controller) = session.registered_controller.clone() else {
        return Vec::new();
    };
    if let Some(decision) = session.renew_current_lease(now_ms) {
        let mut responses = vec![super::outbound::controller_lease(
            &decision,
            decision.granted_lease_id.as_deref().unwrap_or_default(),
        )];
        let revision = session
            .desktop_settings()
            .and_then(|settings| settings.profile_snapshot().ok())
            .map(|profile| format!("desktop:{}", profile.revision()));
        if revision.as_deref() != session.settings_revision()
            && let Some(snapshot) = accepted_snapshot(session, now_ms)
        {
            responses.push(snapshot);
        }
        append_pending(session, effects, &mut responses);
        return responses;
    }
    let mut responses = Vec::new();
    if session.controller_lease_id.is_some() {
        responses.push(super::controller::observer_response(
            session,
            &controller,
            now_ms,
            crate::protocol::TargetSelectionStatus::Available,
        ));
        if let Some(generation) = session.release_controller_lease() {
            let _ = effects.mark_tray_connection_disconnected(generation);
        }
    }
    let target = crate::settings::BrowserTarget::from_controller(&controller);
    let selected = session
        .desktop_settings()
        .and_then(|settings| settings.browser_targets().ok())
        .is_some_and(|registry| {
            target.is_ok_and(|target| registry.selected_target_id() == Some(target.id.as_str()))
        });
    if selected && !session.connection_retired {
        let acquire = crate::protocol::ExtensionControllerAcquireMessage {
            protocol_version: PROTOCOL_VERSION,
            connection_id: controller.connection_id,
            browser_family: controller.browser_family,
            profile_key: controller.profile_key,
            document_id: controller.document_id,
            requested_at_epoch_ms: now_ms,
            reason: crate::protocol::ControllerAcquireReason::InitialConnect,
        };
        responses.extend(super::controller::controller_acquire_responses(
            session, &acquire, effects,
        ));
    }
    responses
}

const MAX_RECONNECT_MEDIA_PER_KIND: usize = 16;

pub(super) fn append_pending(
    session: &mut NativeHostSession,
    effects: &mut impl super::NativeHostSideEffects,
    responses: &mut Vec<Value>,
) {
    let Some(lease_id) = session.controller_lease_id.clone() else {
        return;
    };
    // Bound one delivery batch and forget acknowledged media instead of retaining every ID
    // for the lifetime of the browser connection. Later batches become visible after ACK.
    let mut pending = std::collections::BTreeSet::new();
    if let Ok(items) = effects.pending_screenshots_for_reconnect() {
        for item in items.into_iter().take(MAX_RECONNECT_MEDIA_PER_KIND) {
            pending.insert(format!("screenshot:{}", item.capture_id));
            if session
                .announced_media
                .insert(format!("screenshot:{}", item.capture_id))
            {
                responses.push(super::reconnect::screenshot_start_message(&lease_id, item));
            }
        }
    }
    if let Ok(items) = effects.pending_recordings_for_reconnect() {
        for item in items.into_iter().take(MAX_RECONNECT_MEDIA_PER_KIND) {
            let key = format!("recording:{}", item.recording_id);
            pending.insert(key.clone());
            if !session.announced_media.contains(&key)
                && let Ok(message) = super::reconnect::recording_stopped_message(
                    &lease_id,
                    item,
                    session.recording_formats.cursor_trajectory,
                    session.recording_formats.recording_events,
                )
            {
                session.announced_media.insert(key);
                responses.push(message);
            }
        }
    }
    super::reconnect::append_history_notifications(
        session,
        effects,
        responses,
        &mut pending,
        &lease_id,
    );
    session.announced_media.retain(|key| pending.contains(key));
}

pub(super) fn accepted_snapshot(session: &mut NativeHostSession, now_ms: u64) -> Option<Value> {
    let profile = session.desktop_settings()?.profile_snapshot().ok()?;
    let controller_lease_id = session.controller_lease_id.clone()?;
    let settings = ExtensionSettingsSyncMessage {
        protocol_version: PROTOCOL_VERSION,
        controller_lease_id: controller_lease_id.clone(),
        revision: format!("desktop:{}", profile.revision()),
        schema_version: SETTINGS_SCHEMA_VERSION,
        generated_at_epoch_ms: now_ms.max(1),
        settings: desktop_settings_snapshot(&profile),
    };
    let value = serde_json::to_value(&settings.settings).ok()?;
    let snapshot = crate::settings::AcceptedSettingsSnapshot {
        controller_lease_id,
        revision: settings.revision.clone(),
        schema_version: SETTINGS_SCHEMA_VERSION,
        generated_at_epoch_ms: now_ms.max(1),
        max_recording_duration_ms: profile
            .manual_recording_settings()
            .map_or(3_600_000, |preferences| preferences.max_duration_ms),
        telemetry: crate::settings::AcceptedRecordingTelemetrySettings::default(),
        settings: value,
    };
    session
        .update_shared_accepted_state(|store, _, _| store.accept(snapshot).map(|_| ()))
        .ok()?
        .ok()?;
    Some(super::outbound::settings_accepted(&settings, now_ms))
}

#[cfg(test)]
mod tests;
