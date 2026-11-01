//! Native-host controller lease acquire responses.

use serde_json::Value;

use crate::protocol::{
    ExtensionControllerAcquireMessage, NativeControllerIdentity, OperationalErrorKind,
};

use super::{
    HandshakeFailure, NativeHostSession, NativeHostSideEffects, controller_lease_id,
    handshake_failure,
    outbound::controller_lease,
    reconnect::{append_pending_recording_notifications, append_pending_screenshot_notifications},
};

pub(super) fn controller_acquire_responses(
    session: &mut NativeHostSession,
    acquire: &ExtensionControllerAcquireMessage,
    effects: &mut impl NativeHostSideEffects,
) -> Vec<Value> {
    let now_epoch_ms = session.trusted_epoch_ms(acquire.requested_at_epoch_ms);
    if !session.handshake_accepted {
        return vec![handshake_failure(
            HandshakeFailure {
                kind: OperationalErrorKind::MalformedMessage,
                message: "extension.hello must be accepted before controller acquire",
            },
            Some(&acquire.connection_id),
        )];
    }
    let Some(extension_id) = session.extension_id.clone() else {
        return vec![handshake_failure(
            HandshakeFailure {
                kind: OperationalErrorKind::MalformedMessage,
                message: "accepted extension.hello is missing extension id",
            },
            Some(&acquire.connection_id),
        )];
    };
    let controller = NativeControllerIdentity {
        extension_id,
        browser_family: acquire.browser_family.clone(),
        profile_key: acquire.profile_key.clone(),
        document_id: acquire.document_id.clone(),
        connection_id: acquire.connection_id.clone(),
    };
    session.registered_controller = Some(controller.clone());
    #[cfg(any(windows, test))]
    if let Some(response) = desktop_selection_response(session, &controller, now_epoch_ms) {
        return vec![response];
    }
    let requested_lease_id = if session.desktop_owned() {
        let mut nonce = [0_u8; 16];
        if getrandom::fill(&mut nonce).is_err() {
            return vec![handshake_failure(
                HandshakeFailure {
                    kind: OperationalErrorKind::StorageUnavailable,
                    message: "controller lease entropy is unavailable",
                },
                None,
            )];
        }
        format!("lease-{:032x}", u128::from_le_bytes(nonce))
    } else {
        controller_lease_id(&acquire.connection_id)
    };
    let Ok(decision) = session.acquire_controller_lease(
        controller,
        if session.desktop_owned() {
            &crate::protocol::ControllerAcquireReason::UserRequestedTakeover
        } else {
            &acquire.reason
        },
        now_epoch_ms,
        requested_lease_id.clone(),
    ) else {
        return vec![handshake_failure(
            HandshakeFailure {
                kind: OperationalErrorKind::MalformedMessage,
                message: "controller lease request is invalid",
            },
            Some(&acquire.connection_id),
        )];
    };
    granted_controller_responses(
        session,
        effects,
        &decision,
        &requested_lease_id,
        now_epoch_ms,
    )
}

fn granted_controller_responses(
    session: &mut NativeHostSession,
    effects: &mut impl NativeHostSideEffects,
    decision: &crate::ipc::ControllerLeaseDecision,
    requested_lease_id: &str,
    now_epoch_ms: u64,
) -> Vec<Value> {
    let mut responses = vec![controller_lease(decision, requested_lease_id)];
    if let Some(lease_id) = decision.granted_lease_id.clone() {
        #[cfg(any(windows, test))]
        if session.desktop_owned()
            && let Ok(target) =
                crate::settings::BrowserTarget::from_controller(&decision.controller)
        {
            effects.bind_browser_target(&target.id);
        }
        session.controller_lease_id = Some(lease_id);
        session.active_controller = Some(decision.controller.clone());
        #[cfg(any(windows, test))]
        if let Some(snapshot) = super::desktop_settings::accepted_snapshot(session, now_epoch_ms) {
            responses.push(snapshot);
        }
        let generation = session.begin_tray_connection_projection();
        let _ = effects.mark_tray_connection_connected(generation);
        #[cfg(any(windows, test))]
        if session.desktop_owned() {
            super::desktop_settings::append_pending(session, effects, &mut responses);
            return responses;
        }
        append_pending_screenshot_notifications(
            &mut responses,
            requested_lease_id,
            now_epoch_ms,
            effects,
        );
        append_pending_recording_notifications(
            &mut responses,
            requested_lease_id,
            now_epoch_ms,
            session.recording_formats.cursor_trajectory,
            session.recording_formats.recording_events,
            effects,
        );
    }
    responses
}

#[cfg(any(windows, test))]
fn desktop_selection_response(
    session: &mut NativeHostSession,
    controller: &NativeControllerIdentity,
    now_epoch_ms: u64,
) -> Option<Value> {
    if session.connection_was_replaced(now_epoch_ms) {
        return Some(observer_response(
            session,
            controller,
            now_epoch_ms,
            crate::protocol::TargetSelectionStatus::Available,
        ));
    }
    if let Some(settings) = session.desktop_settings() {
        let registered = settings.register_browser_target(controller);
        let registry = settings.browser_targets();
        let (Ok(target), Ok(registry)) = (registered, registry) else {
            return Some(handshake_failure(
                HandshakeFailure {
                    kind: OperationalErrorKind::StorageUnavailable,
                    message: "browser target registry is unavailable",
                },
                None,
            ));
        };
        if registry.selected_target_id() != Some(target.id.as_str()) {
            let selection = if registry.selected_target_id().is_none() {
                crate::protocol::TargetSelectionStatus::SelectionRequired
            } else {
                crate::protocol::TargetSelectionStatus::Available
            };
            return Some(observer_response(
                session,
                controller,
                now_epoch_ms,
                selection,
            ));
        }
    }
    None
}

#[cfg(any(windows, test))]
pub(super) fn observer_response(
    session: &NativeHostSession,
    controller: &NativeControllerIdentity,
    now_ms: u64,
    selection: crate::protocol::TargetSelectionStatus,
) -> Value {
    let decision = session.observer_decision(controller, now_ms);
    match decision {
        Some(decision) => super::outbound::controller_lease_with_selection(
            &decision,
            &controller_lease_id(&controller.connection_id),
            selection,
        ),
        None => handshake_failure(
            HandshakeFailure {
                kind: OperationalErrorKind::StorageUnavailable,
                message: "controller observer projection is unavailable",
            },
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use crate::{
        app::responses::{
            NativeHostSession, effects::NoopNativeHostSideEffects,
            responses_for_message_with_effects,
        },
        protocol::{WireMessage, parse_wire_message},
    };

    #[test]
    fn controller_acquire_fails_closed_without_handshake_extension_id() -> Result<(), Box<dyn Error>>
    {
        let mut session = NativeHostSession::default();
        session.handshake_accepted = true;
        let message = parse_wire_message(
            br#"{
                "type": "extension.controller.acquire",
                "protocolVersion": 2,
                "connectionId": "connection-1",
                "browserFamily": "chrome",
                "profileKey": "profile-1",
                "documentId": "document-1",
                "requestedAtEpochMs": 1000,
                "reason": "initial-connect"
            }"#,
        )?;
        let WireMessage::ExtensionControllerAcquire(acquire) = &message else {
            return Err("expected controller acquire message".into());
        };
        let value = serde_json::to_value(acquire)?;
        let mut effects = NoopNativeHostSideEffects;

        let responses =
            responses_for_message_with_effects(&mut session, &message, &value, &mut effects);

        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0]["type"], "app.operation.failed");
        assert_eq!(responses[0]["operation"], "handshake");
        assert_eq!(responses[0]["operationId"], "connection-1");
        assert_eq!(responses[0]["error"]["code"], "malformed-message");
        assert!(session.controller_lease_id.is_none());
        assert!(session.active_controller.is_none());
        Ok(())
    }
}
