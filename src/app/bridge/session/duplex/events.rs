//! Outbound resident-agent event projection for duplex pipe sessions.

use serde_json::Value;

use crate::{
    app::{
        resident::AgentOutboundEvent,
        responses::{
            NativeHostSession, responses_for_command_event, responses_for_command_failure,
            responses_for_synced_tray_action, responses_for_tray_action,
        },
    },
    tray::{TrayActionKind, TrayActionRequest},
};

pub(super) fn responses_for_agent_event(
    session: &mut NativeHostSession,
    event: AgentOutboundEvent,
) -> Vec<Value> {
    match event {
        AgentOutboundEvent::CommandEvent(event) => responses_for_command_event(session, *event),
        AgentOutboundEvent::CommandFailure(failure) => {
            responses_for_command_failure(session, &failure)
        }
        AgentOutboundEvent::TrayActionRequested {
            action_id,
            invocation_id,
            requested_at_epoch_ms,
        } => responses_for_synced_tray_action(
            session,
            &action_id,
            true,
            invocation_id,
            requested_at_epoch_ms,
        )
        .unwrap_or_default(),
        AgentOutboundEvent::OpenSettingsRequested {
            invocation_id,
            requested_at_epoch_ms,
        } => responses_for_tray_action(
            session,
            &TrayActionRequest {
                action_id: "open-settings".to_owned(),
                invocation_id,
                requested_at_epoch_ms,
                kind: TrayActionKind::OpenSettings,
            },
        ),
    }
}
