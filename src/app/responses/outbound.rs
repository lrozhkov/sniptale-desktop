//! Typed Native Messaging outbound serialization.

use serde_json::Value;

#[cfg(any(windows, test))]
use super::NativeHostCommandEvent;
#[cfg(any(windows, test))]
use crate::protocol::{AcceptedOperation, AppCommandAcceptedMessage};
use crate::{
    app::responses::runtime_install_state,
    ipc::ControllerLeaseDecision,
    protocol::{
        AppControllerLeaseMessage, AppOperationFailedMessage, AppPongMessage,
        AppRecordingStoppedMessage, AppSettingsAcceptedMessage, NativeAppError, NativeAppStatus,
        OperationFailedOperation, OperationalErrorKind, PROTOCOL_VERSION, WireMessage,
    },
    settings::effective_settings_for_sync,
};

#[derive(Clone, Copy)]
pub(super) struct OperationFailure<'a> {
    pub(super) controller_lease_id: Option<&'a str>,
    pub(super) operation: OperationFailedOperation,
    pub(super) operation_id: Option<&'a str>,
    pub(super) phase: &'a str,
    pub(super) kind: OperationalErrorKind,
    pub(super) message: Option<&'a str>,
    pub(super) occurred_at_epoch_ms: u64,
}

#[cfg(any(windows, test))]
pub(super) fn command_accepted(
    controller_lease_id: &str,
    command_id: &str,
    operation: AcceptedOperation,
    accepted_at_epoch_ms: u64,
) -> Value {
    wire_value(WireMessage::AppCommandAccepted(Box::new(
        AppCommandAcceptedMessage {
            protocol_version: PROTOCOL_VERSION,
            controller_lease_id: controller_lease_id.to_owned(),
            command_id: command_id.to_owned(),
            operation,
            accepted_at_epoch_ms: accepted_at_epoch_ms.max(1),
        },
    )))
}

#[cfg(any(windows, test))]
pub(super) fn command_event(event: NativeHostCommandEvent) -> Value {
    match event {
        NativeHostCommandEvent::ScreenshotStart(message) => {
            wire_value(WireMessage::AppScreenshotStart(Box::new(message)))
        }
        NativeHostCommandEvent::RecordingStarted(message) => {
            wire_value(WireMessage::AppRecordingStarted(Box::new(message)))
        }
        NativeHostCommandEvent::RecordingProgress(message) => {
            wire_value(WireMessage::AppRecordingProgress(Box::new(message)))
        }
        NativeHostCommandEvent::RecordingStopped(message) => {
            wire_value(WireMessage::AppRecordingStopped(Box::new(message)))
        }
    }
}

pub(super) fn controller_lease(
    decision: &ControllerLeaseDecision,
    requested_lease_id: &str,
) -> Value {
    wire_value(WireMessage::AppControllerLease(Box::new(
        AppControllerLeaseMessage {
            protocol_version: PROTOCOL_VERSION,
            controller_lease_id: decision
                .granted_lease_id
                .clone()
                .unwrap_or_else(|| requested_lease_id.to_owned()),
            controller: decision.controller.clone(),
            expires_at_epoch_ms: decision.expires_at_epoch_ms,
            status: decision.status.clone(),
        },
    )))
}

pub(super) fn operation_failed(failure: OperationFailure<'_>) -> Value {
    let descriptor = failure.kind.descriptor();
    debug_assert!(failure.kind.is_cataloged());
    debug_assert!(failure.kind.has_cataloged_public_code());
    debug_assert_ne!(descriptor.catalog_key, "");
    wire_value(WireMessage::AppOperationFailed(Box::new(
        AppOperationFailedMessage {
            protocol_version: PROTOCOL_VERSION,
            controller_lease_id: failure.controller_lease_id.map(str::to_owned),
            operation: failure.operation,
            operation_id: failure.operation_id.map(str::to_owned),
            phase: failure.phase.to_owned(),
            error: NativeAppError {
                code: descriptor.public_code,
                message: Some(
                    failure
                        .message
                        .unwrap_or(descriptor.default_message)
                        .to_owned(),
                ),
                recoverable: descriptor.recoverable,
            },
            occurred_at_epoch_ms: failure.occurred_at_epoch_ms.max(1),
        },
    )))
}

pub(super) fn pong(nonce: &str, sent_at_epoch_ms: u64, settings_revision: Option<&str>) -> Value {
    wire_value(WireMessage::AppPong(Box::new(AppPongMessage {
        protocol_version: PROTOCOL_VERSION,
        nonce: nonce.to_owned(),
        sent_at_epoch_ms: sent_at_epoch_ms.saturating_add(1),
        app_status: NativeAppStatus {
            connected_browser: true,
            recording: None,
            last_error: None,
            settings_revision: settings_revision.map(str::to_owned),
            install: runtime_install_state(),
        },
    })))
}

pub(super) fn recording_stopped(message: AppRecordingStoppedMessage) -> Value {
    wire_value(WireMessage::AppRecordingStopped(Box::new(message)))
}

pub(super) fn settings_accepted(
    settings: &crate::protocol::ExtensionSettingsSyncMessage,
    accepted_at_epoch_ms: u64,
) -> Value {
    let effective_settings = effective_settings_for_sync(&settings.settings);
    let warnings = effective_settings.warnings.clone();
    wire_value(WireMessage::AppSettingsAccepted(Box::new(
        AppSettingsAcceptedMessage {
            protocol_version: PROTOCOL_VERSION,
            controller_lease_id: settings.controller_lease_id.clone(),
            revision: settings.revision.clone(),
            schema_version: settings.schema_version,
            accepted_at_epoch_ms: accepted_at_epoch_ms.saturating_add(1).max(1),
            effective_settings,
            warnings,
        },
    )))
}

fn wire_value(message: WireMessage) -> Value {
    match serde_json::to_value(message) {
        Ok(value) => value,
        Err(error) => {
            unreachable!("protocol wire DTO serialization should not fail: {error}")
        }
    }
}
