//! Extension-command projection into recording-owned domain requests.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::{
    protocol::{
        DeliveryRoute, ExtensionRecordingControlCommandMessage,
        ExtensionRecordingStartCommandMessage, OperationAuthority,
    },
    recording::{RecordingControlRequest, RecordingDeliverySession, RecordingStartRequest},
};

pub(super) fn extension_start_request(
    command: &ExtensionRecordingStartCommandMessage,
) -> RecordingStartRequest {
    RecordingStartRequest {
        operation_id: command.command_id.clone(),
        source: command.source.clone(),
        authority: OperationAuthority::ExtensionManaged {
            settings_revision: command.settings_revision.clone(),
        },
        delivery_route: DeliveryRoute::ExtensionTransfer,
        settings_revision: command.settings_revision.clone(),
        requested_quality: command.requested_quality.clone(),
        include_cursor: true,
        open_editor: command.open_editor,
        requested_at_epoch_ms: command.requested_at_epoch_ms,
        delivery_session: RecordingDeliverySession {
            controller_lease_id: Some(command.controller_lease_id.clone()),
        },
    }
}

pub(super) fn extension_control_request(
    command: &ExtensionRecordingControlCommandMessage,
) -> RecordingControlRequest {
    RecordingControlRequest {
        operation_id: command.command_id.clone(),
        recording_id: command.recording_id.clone(),
        control: command.control.clone(),
        requested_at_epoch_ms: command.requested_at_epoch_ms,
        delivery_session: RecordingDeliverySession {
            controller_lease_id: Some(command.controller_lease_id.clone()),
        },
    }
}

pub(super) fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1, |duration| {
            duration.as_millis().try_into().unwrap_or(u64::MAX)
        })
        .max(1)
}
