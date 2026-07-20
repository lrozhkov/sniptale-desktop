//! Native-host outbound tray event messages.

use serde_json::{Value, json};

use super::NativeHostSession;
use crate::{
    protocol::{
        AppOpenSettingsRequestedMessage, AppTrayActionRequestedMessage, OpenSettingsSection,
        PROTOCOL_VERSION, WireMessage,
    },
    tray::{TrayActionError, TrayActionKind, TrayActionRequest},
};

pub(crate) fn responses_for_synced_tray_action(
    session: &NativeHostSession,
    action_id: &str,
    connected: bool,
    invocation_id: impl Into<String>,
    requested_at_epoch_ms: u64,
) -> Result<Vec<Value>, TrayActionError> {
    let registry = session
        .tray_registry()
        .ok_or(TrayActionError::Disconnected)?;
    let request =
        registry.request_action(action_id, connected, invocation_id, requested_at_epoch_ms)?;
    Ok(responses_for_tray_action(session, &request))
}

pub(crate) fn responses_for_tray_action(
    session: &NativeHostSession,
    request: &TrayActionRequest,
) -> Vec<Value> {
    let Some(controller_lease_id) = session.controller_lease_id() else {
        return Vec::new();
    };
    match request.kind {
        TrayActionKind::OpenSettings => vec![app_open_settings_requested(
            controller_lease_id,
            &request.invocation_id,
            request.requested_at_epoch_ms,
        )],
        TrayActionKind::OpenSaveFolder | TrayActionKind::Quit => Vec::new(),
        TrayActionKind::CaptureScreenshot
        | TrayActionKind::StartRecording
        | TrayActionKind::StopRecording
        | TrayActionKind::PauseRecording
        | TrayActionKind::ResumeRecording
        | TrayActionKind::OpenGallery
        | TrayActionKind::OpenVideoEditor => {
            vec![app_tray_action_requested(controller_lease_id, request)]
        }
    }
}

fn app_tray_action_requested(controller_lease_id: &str, request: &TrayActionRequest) -> Value {
    json!(WireMessage::AppTrayActionRequested(Box::new(
        AppTrayActionRequestedMessage {
            protocol_version: PROTOCOL_VERSION,
            controller_lease_id: controller_lease_id.to_owned(),
            action_id: request.action_id.clone(),
            requested_at_epoch_ms: request.requested_at_epoch_ms,
            invocation_id: request.invocation_id.clone(),
        }
    )))
}

fn app_open_settings_requested(
    controller_lease_id: &str,
    invocation_id: &str,
    requested_at_epoch_ms: u64,
) -> Value {
    json!(WireMessage::AppOpenSettingsRequested(Box::new(
        AppOpenSettingsRequestedMessage {
            protocol_version: PROTOCOL_VERSION,
            controller_lease_id: controller_lease_id.to_owned(),
            section: OpenSettingsSection::NativeApp,
            requested_at_epoch_ms,
            invocation_id: invocation_id.to_owned(),
        }
    )))
}
