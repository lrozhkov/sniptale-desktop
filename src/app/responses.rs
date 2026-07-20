//! Native Messaging protocol responses for the stdio bridge.

mod clock;
mod command;
mod controller;
mod effects;
mod handshake;
mod hello;
mod hotkeys;
#[cfg(any(windows, test))]
mod media_projection;
mod outbound;
mod ping;
mod reconnect;
mod session;
mod settings_sync;
mod transfer;
#[cfg(any(windows, test))]
mod tray;
mod tray_result;

use serde_json::Value;

use crate::{
    protocol::{AcceptedOperation, OperationFailedOperation, WireMessage},
    recording::RecordingStartPolicy,
};

use self::command::{CommandFailureInputs, command_response};
use self::controller::controller_acquire_responses;
#[cfg(any(windows, test))]
pub(crate) use self::effects::NativeHostTransferChunk;
#[cfg(any(windows, test))]
pub(crate) use self::effects::NativeHostTransferChunkError;
#[cfg(any(not(windows), test))]
use self::effects::NoopNativeHostSideEffects;
#[cfg(any(windows, test))]
pub(crate) use self::effects::extension_screenshot_capture_request;
#[cfg(any(windows, test))]
pub(crate) use self::effects::{NativeHostCommandEvent, NativeHostCommandFailureEvent};
pub(crate) use self::effects::{
    NativeHostCommandOutcome, NativeHostSideEffectError, NativeHostSideEffects,
};
use self::handshake::{HandshakeFailure, hello_compatibility_failure};
#[cfg(not(test))]
use self::hello::app_hello;
#[cfg(test)]
pub(in crate::app) use self::hello::{
    CapabilityFacts, app_hello, capabilities_from_facts, install_state,
};
pub(in crate::app) use self::hello::{runtime_capability_snapshot, runtime_install_state};
#[cfg(any(windows, test))]
pub(crate) use self::media_projection::screenshot_command_event;
use self::outbound::{OperationFailure, operation_failed};
use self::ping::ping_responses;
pub(crate) use self::session::NativeHostSession;
#[cfg(any(windows, test))]
pub(crate) use self::session::NativeHostSharedState;
use self::settings_sync::settings_sync_message_responses;
use self::transfer::{TransferInputs, transfer_response};
#[cfg(any(windows, test))]
pub(crate) use self::tray::{responses_for_synced_tray_action, responses_for_tray_action};

#[cfg(any(not(windows), test))]
pub(crate) fn responses_for_message(
    session: &mut NativeHostSession,
    message: &WireMessage,
    value: &Value,
) -> Vec<Value> {
    let mut effects = NoopNativeHostSideEffects;
    responses_for_message_with_effects(session, message, value, &mut effects)
}

pub(crate) fn responses_for_message_with_effects(
    session: &mut NativeHostSession,
    message: &WireMessage,
    value: &Value,
    effects: &mut impl NativeHostSideEffects,
) -> Vec<Value> {
    if let Some(inputs) = TransferInputs::from_message(message) {
        return transfer_responses(session, inputs, effects);
    }

    match message {
        WireMessage::ExtensionHello(hello) => {
            if let Some(failure) = hello_compatibility_failure(hello) {
                session.handshake_accepted = false;
                return vec![handshake_failure(failure, None)];
            }
            session.handshake_accepted = true;
            session.extension_id = Some(hello.extension_id.clone());
            vec![app_hello()]
        }
        WireMessage::ExtensionControllerAcquire(acquire) => {
            controller_acquire_responses(session, acquire, effects)
        }
        WireMessage::ExtensionSettingsSync(settings) => {
            settings_sync_message_responses(session, settings, value, effects)
        }
        WireMessage::ExtensionPing(ping) => {
            ping_responses(session, &ping.nonce, ping.sent_at_epoch_ms)
        }
        WireMessage::ExtensionTrayActionResult(result) => {
            tray_result::handle_tray_action_result(result.accepted, effects);
            Vec::new()
        }
        WireMessage::ExtensionScreenshotCaptureCommand(command) => command_responses(
            session,
            CommandFailureInputs {
                controller_lease_id: &command.controller_lease_id,
                operation: AcceptedOperation::Screenshot,
                command_id: &command.command_id,
                settings_revision: Some(&command.settings_revision),
                requested_at_epoch_ms: command.requested_at_epoch_ms,
            },
            || effects.start_screenshot_capture(command),
        ),
        WireMessage::ExtensionRecordingStartCommand(command) => {
            let policy = RecordingStartPolicy {
                max_duration_ms: session
                    .max_recording_duration_ms()
                    .unwrap_or_else(|| RecordingStartPolicy::default().max_duration_ms),
                telemetry: session
                    .recording_telemetry_policy()
                    .unwrap_or_else(|| RecordingStartPolicy::default().telemetry),
            };
            command_responses(
                session,
                CommandFailureInputs {
                    controller_lease_id: &command.controller_lease_id,
                    operation: AcceptedOperation::Recording,
                    command_id: &command.command_id,
                    settings_revision: Some(&command.settings_revision),
                    requested_at_epoch_ms: command.requested_at_epoch_ms,
                },
                || effects.start_recording(command, policy),
            )
        }
        WireMessage::ExtensionRecordingControlCommand(command) => command_responses(
            session,
            CommandFailureInputs {
                controller_lease_id: &command.controller_lease_id,
                operation: AcceptedOperation::Recording,
                command_id: &command.command_id,
                settings_revision: None,
                requested_at_epoch_ms: command.requested_at_epoch_ms,
            },
            || effects.control_recording(command),
        ),
        _ => Vec::new(),
    }
}

#[cfg(any(windows, test))]
pub(crate) fn responses_for_command_event(
    session: &NativeHostSession,
    event: NativeHostCommandEvent,
) -> Vec<Value> {
    if !session
        .validate_active_lease(event.controller_lease_id(), session.current_epoch_ms())
        .is_valid()
    {
        return Vec::new();
    }
    vec![outbound::command_event(event)]
}

#[cfg(any(windows, test))]
pub(crate) fn responses_for_command_failure(
    session: &NativeHostSession,
    failure: &NativeHostCommandFailureEvent,
) -> Vec<Value> {
    let Some(controller_lease_id) = failure.controller_lease_id.as_deref() else {
        return Vec::new();
    };
    if !session
        .validate_active_lease(controller_lease_id, session.current_epoch_ms())
        .is_valid()
    {
        return Vec::new();
    }
    vec![operation_failed(OperationFailure {
        controller_lease_id: Some(controller_lease_id),
        operation: failure.operation,
        operation_id: failure.operation_id.as_deref(),
        phase: failure.phase,
        kind: failure.kind,
        message: None,
        occurred_at_epoch_ms: failure.occurred_at_epoch_ms,
    })]
}

fn command_responses(
    session: &mut NativeHostSession,
    inputs: CommandFailureInputs<'_>,
    dispatch: impl FnOnce() -> Result<NativeHostCommandOutcome, NativeHostSideEffectError>,
) -> Vec<Value> {
    let now_epoch_ms = session.trusted_epoch_ms(inputs.requested_at_epoch_ms);
    let inputs = CommandFailureInputs {
        requested_at_epoch_ms: now_epoch_ms,
        ..inputs
    };
    let lease_validation = session.validate_active_lease(inputs.controller_lease_id, now_epoch_ms);
    let settings_revision = session.settings_revision().map(str::to_owned);
    command_response(
        &mut session.command_results,
        lease_validation,
        settings_revision.as_deref(),
        inputs,
        dispatch,
    )
}

fn transfer_responses(
    session: &NativeHostSession,
    inputs: TransferInputs<'_>,
    effects: &mut impl NativeHostSideEffects,
) -> Vec<Value> {
    let now_epoch_ms = session.current_epoch_ms();
    let lease_validation = session.validate_active_lease(inputs.controller_lease_id, now_epoch_ms);
    transfer_response(lease_validation, now_epoch_ms, inputs, effects)
}

fn handshake_failure(failure: HandshakeFailure, operation_id: Option<&str>) -> Value {
    operation_failed(OperationFailure {
        controller_lease_id: None,
        operation: OperationFailedOperation::Handshake,
        operation_id,
        phase: "version-negotiation",
        kind: failure.kind,
        message: Some(failure.message),
        occurred_at_epoch_ms: 1,
    })
}

fn controller_lease_id(connection_id: &str) -> String {
    format!("lease:{connection_id}")
}
