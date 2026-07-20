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
    let requested_lease_id = controller_lease_id(&acquire.connection_id);
    let Ok(decision) = session.acquire_controller_lease(
        controller,
        &acquire.reason,
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
    let mut responses = vec![controller_lease(&decision, &requested_lease_id)];
    if let Some(lease_id) = decision.granted_lease_id.clone() {
        session.controller_lease_id = Some(lease_id);
        session.active_controller = Some(decision.controller.clone());
        let generation = session.begin_tray_connection_projection();
        let _ = effects.mark_tray_connection_connected(generation);
        append_pending_screenshot_notifications(
            &mut responses,
            &requested_lease_id,
            now_epoch_ms,
            effects,
        );
        append_pending_recording_notifications(
            &mut responses,
            &requested_lease_id,
            now_epoch_ms,
            effects,
        );
    }
    responses
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
                "protocolVersion": 1,
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
