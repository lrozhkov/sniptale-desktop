//! Resident agent outbound event routing to the active native-host session.

use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use crate::app::responses::{NativeHostCommandEvent, NativeHostCommandFailureEvent};
#[cfg(any(windows, test))]
use crate::tray::{ShellActionId, TrayActionError, TrayActionRegistry};

const AGENT_EVENT_QUEUE_CAPACITY: usize = 16;
const AGENT_EVENT_MAX_SUBSCRIBERS: usize = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum AgentOutboundEvent {
    CommandEvent(Box<NativeHostCommandEvent>),
    CommandFailure(Box<NativeHostCommandFailureEvent>),
    TrayActionRequested {
        action_id: String,
        invocation_id: String,
        requested_at_epoch_ms: u64,
    },
    OpenSettingsRequested {
        invocation_id: String,
        requested_at_epoch_ms: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AgentEventPublishResult {
    Queued,
    NoSubscriber,
    QueueFull,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AgentEventHub {
    subscribers: Arc<Mutex<Vec<SyncSender<AgentOutboundEvent>>>>,
    next_invocation: Arc<AtomicU64>,
}

impl AgentEventHub {
    pub(crate) fn subscribe(&self) -> Receiver<AgentOutboundEvent> {
        let (sender, receiver) = mpsc::sync_channel(AGENT_EVENT_QUEUE_CAPACITY);
        if let Ok(mut subscribers) = self.subscribers.lock() {
            if subscribers.len() >= AGENT_EVENT_MAX_SUBSCRIBERS {
                subscribers.remove(0);
            }
            subscribers.push(sender);
        }
        receiver
    }

    pub(crate) fn publish_hotkey_action(&self, action_id: &str) -> AgentEventPublishResult {
        self.publish_action("hotkey", action_id)
    }

    pub(crate) fn publish_tray_menu_action(&self, action_id: &str) -> AgentEventPublishResult {
        self.publish_action("tray", action_id)
    }

    #[cfg(any(windows, test))]
    pub(crate) fn publish_tray_extension_intent(
        &self,
        registry: &TrayActionRegistry,
        shell_action: ShellActionId,
        connected: bool,
    ) -> Result<AgentEventPublishResult, TrayActionError> {
        let sequence = self.next_invocation.fetch_add(1, Ordering::Relaxed) + 1;
        let request = registry.request_extension_intent(
            shell_action,
            connected,
            format!("tray-{sequence}"),
            system_epoch_ms(),
        )?;
        Ok(self.publish(&AgentOutboundEvent::TrayActionRequested {
            action_id: request.action_id,
            invocation_id: request.invocation_id,
            requested_at_epoch_ms: request.requested_at_epoch_ms,
        }))
    }

    pub(crate) fn publish_open_settings(&self) -> AgentEventPublishResult {
        let sequence = self.next_invocation.fetch_add(1, Ordering::Relaxed) + 1;
        self.publish(&AgentOutboundEvent::OpenSettingsRequested {
            invocation_id: format!("tray-{sequence}"),
            requested_at_epoch_ms: system_epoch_ms(),
        })
    }

    pub(crate) fn publish_command_event(
        &self,
        event: NativeHostCommandEvent,
    ) -> AgentEventPublishResult {
        self.publish(&AgentOutboundEvent::CommandEvent(Box::new(event)))
    }

    pub(crate) fn publish_command_failure(
        &self,
        failure: NativeHostCommandFailureEvent,
    ) -> AgentEventPublishResult {
        self.publish(&AgentOutboundEvent::CommandFailure(Box::new(failure)))
    }

    fn publish_action(&self, source: &str, action_id: &str) -> AgentEventPublishResult {
        let sequence = self.next_invocation.fetch_add(1, Ordering::Relaxed) + 1;
        self.publish(&AgentOutboundEvent::TrayActionRequested {
            action_id: action_id.to_owned(),
            invocation_id: format!("{source}-{sequence}"),
            requested_at_epoch_ms: system_epoch_ms(),
        })
    }

    #[cfg(test)]
    pub(crate) fn publish(&self, event: &AgentOutboundEvent) -> AgentEventPublishResult {
        self.broadcast(event)
    }

    #[cfg(not(test))]
    fn publish(&self, event: &AgentOutboundEvent) -> AgentEventPublishResult {
        self.broadcast(event)
    }

    fn broadcast(&self, event: &AgentOutboundEvent) -> AgentEventPublishResult {
        let Ok(mut subscribers) = self.subscribers.lock() else {
            return AgentEventPublishResult::NoSubscriber;
        };
        if subscribers.is_empty() {
            return AgentEventPublishResult::NoSubscriber;
        }

        let mut queued = false;
        let mut saturated = false;
        let mut retained = Vec::with_capacity(subscribers.len());
        for sender in subscribers.drain(..) {
            match sender.try_send(event.clone()) {
                Ok(()) => {
                    queued = true;
                    retained.push(sender);
                }
                Err(TrySendError::Full(_)) => {
                    saturated = true;
                    retained.push(sender);
                }
                Err(TrySendError::Disconnected(_)) => {}
            }
        }
        *subscribers = retained;

        if queued {
            AgentEventPublishResult::Queued
        } else if saturated {
            AgentEventPublishResult::QueueFull
        } else {
            AgentEventPublishResult::NoSubscriber
        }
    }
}

fn system_epoch_ms() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_millis().try_into().unwrap_or(u64::MAX),
        Err(_) => 1,
    }
    .max(1)
}

#[cfg(test)]
mod tests {
    use super::{AgentEventHub, AgentEventPublishResult, AgentOutboundEvent};

    #[test]
    fn subscribe_broadcasts_to_concurrent_outbound_sessions() {
        let hub = AgentEventHub::default();
        let first = hub.subscribe();
        let second = hub.subscribe();

        assert_eq!(
            hub.publish(&AgentOutboundEvent::TrayActionRequested {
                action_id: "capture-screen".to_owned(),
                invocation_id: "hotkey-1".to_owned(),
                requested_at_epoch_ms: 4_000,
            }),
            AgentEventPublishResult::Queued
        );

        let first_action_id = first.try_recv().map(|event| match event {
            AgentOutboundEvent::TrayActionRequested { action_id, .. } => action_id,
            AgentOutboundEvent::OpenSettingsRequested { .. } => "open-settings".to_owned(),
            AgentOutboundEvent::CommandEvent(_) | AgentOutboundEvent::CommandFailure(_) => {
                "command".to_owned()
            }
        });
        let second_action_id = second.try_recv().map(|event| match event {
            AgentOutboundEvent::TrayActionRequested { action_id, .. } => action_id,
            AgentOutboundEvent::OpenSettingsRequested { .. } => "open-settings".to_owned(),
            AgentOutboundEvent::CommandEvent(_) | AgentOutboundEvent::CommandFailure(_) => {
                "command".to_owned()
            }
        });
        assert_eq!(first_action_id, Ok("capture-screen".to_owned()));
        assert_eq!(second_action_id, Ok("capture-screen".to_owned()));
    }

    #[test]
    fn closed_probe_session_does_not_steal_outbound_events_from_live_session() {
        let hub = AgentEventHub::default();
        let live = hub.subscribe();
        let probe = hub.subscribe();
        drop(probe);

        assert_eq!(
            hub.publish(&AgentOutboundEvent::OpenSettingsRequested {
                invocation_id: "tray-1".to_owned(),
                requested_at_epoch_ms: 4_000,
            }),
            AgentEventPublishResult::Queued
        );

        assert!(matches!(
            live.try_recv(),
            Ok(AgentOutboundEvent::OpenSettingsRequested { .. })
        ));
    }

    #[test]
    fn publish_reports_missing_or_saturated_outbound_session() {
        let hub = AgentEventHub::default();
        assert_eq!(
            hub.publish_open_settings(),
            AgentEventPublishResult::NoSubscriber
        );

        let receiver = hub.subscribe();
        for index in 0..16 {
            assert_eq!(
                hub.publish(&AgentOutboundEvent::TrayActionRequested {
                    action_id: format!("capture-{index}"),
                    invocation_id: format!("hotkey-{index}"),
                    requested_at_epoch_ms: 4_000,
                }),
                AgentEventPublishResult::Queued
            );
        }
        assert_eq!(
            hub.publish_open_settings(),
            AgentEventPublishResult::QueueFull
        );
        drop(receiver);
        assert_eq!(
            hub.publish_open_settings(),
            AgentEventPublishResult::NoSubscriber
        );
    }
}
