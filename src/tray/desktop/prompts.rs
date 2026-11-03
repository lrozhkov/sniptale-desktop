//! One bounded application-owned prompt. Results are checked by Rust before use.

use serde::{Deserialize, Serialize};
use std::{
    sync::{Condvar, Mutex, mpsc},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
pub(crate) enum PromptContent {
    Connection {
        extension: String,
        launcher: String,
        category: String,
    },
    Destination {
        local_saved: bool,
        targets: Vec<String>,
    },
    QuitRecording,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptView {
    pub(crate) generation: u64,
    pub(crate) content: PromptContent,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PromptReply {
    pub(crate) generation: u64,
    pub(crate) choice: u8,
}

#[derive(Debug)]
struct Pending {
    view: PromptView,
    deadline: Instant,
    sender: Option<mpsc::SyncSender<u8>>,
}

#[derive(Debug, Default)]
pub(crate) struct PromptState {
    pub(crate) serial: Mutex<()>,
    pending: Mutex<(u64, Option<Pending>)>,
    closed: Condvar,
}

impl PromptState {
    pub(crate) fn begin(
        &self,
        content: PromptContent,
        timeout: Duration,
    ) -> Option<mpsc::Receiver<u8>> {
        let mut state = self.pending.lock().ok()?;
        if state.1.is_some() {
            return None;
        }
        let generation = state.0.checked_add(1)?;
        let (sender, receiver) = mpsc::sync_channel(1);
        state.0 = generation;
        state.1 = Some(Pending {
            view: PromptView {
                generation,
                content,
            },
            deadline: Instant::now().checked_add(timeout)?,
            sender: Some(sender),
        });
        Some(receiver)
    }

    pub(crate) fn snapshot(&self) -> Option<PromptView> {
        self.pending
            .lock()
            .ok()?
            .1
            .as_ref()
            .map(|pending| pending.view.clone())
    }

    pub(crate) fn reply(&self, reply: PromptReply) -> bool {
        let Ok(mut state) = self.pending.lock() else {
            return false;
        };
        let Some(pending) = state.1.as_mut() else {
            return false;
        };
        let limit = match &pending.view.content {
            PromptContent::Connection { .. } | PromptContent::QuitRecording => 1,
            PromptContent::Destination { targets, .. } => {
                u8::try_from(targets.len() + 1).unwrap_or(0)
            }
        };
        if reply.generation != pending.view.generation
            || reply.choice > limit
            || Instant::now() >= pending.deadline
        {
            return false;
        }
        pending
            .sender
            .take()
            .is_some_and(|sender| sender.try_send(reply.choice).is_ok())
    }

    pub(crate) fn dismiss(&self) {
        if let Ok(mut state) = self.pending.lock()
            && let Some(pending) = state.1.as_mut()
            && let Some(sender) = pending.sender.take()
        {
            let _ = sender.try_send(0);
        }
    }

    pub(crate) fn end(&self) {
        if let Ok(mut state) = self.pending.lock() {
            state.1 = None;
            self.closed.notify_all();
        }
    }

    pub(crate) fn wait_closed(&self, timeout: Duration) -> bool {
        let Ok(state) = self.pending.lock() else {
            return false;
        };
        self.closed
            .wait_timeout_while(state, timeout, |state| state.1.is_some())
            .is_ok_and(|(state, _)| state.1.is_none())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dismissal_preserves_cancel_and_destination_choices_are_bounded()
    -> Result<(), Box<dyn std::error::Error>> {
        let state = PromptState::default();
        let _serial = state.serial.try_lock().map_err(|_| "serial")?;
        assert!(state.serial.try_lock().is_err());
        let receiver = state
            .begin(
                PromptContent::Destination {
                    local_saved: false,
                    targets: vec!["Browser".to_owned()],
                },
                Duration::from_secs(30),
            )
            .ok_or("begin")?;
        assert!(
            state
                .begin(PromptContent::QuitRecording, Duration::from_secs(30))
                .is_none()
        );
        let generation = state.snapshot().ok_or("snapshot")?.generation;
        assert!(!state.reply(PromptReply {
            generation,
            choice: 3
        }));
        assert!(!state.wait_closed(Duration::ZERO));
        state.dismiss();
        assert_eq!(receiver.try_recv()?, 0);
        assert!(!state.reply(PromptReply {
            generation,
            choice: 2
        }));
        state.end();
        assert!(state.wait_closed(Duration::ZERO));
        let _connection = state
            .begin(
                PromptContent::Connection {
                    extension: "Extension".to_owned(),
                    launcher: "Browser".to_owned(),
                    category: "browser".to_owned(),
                },
                Duration::from_secs(30),
            )
            .ok_or("begin")?;
        state.end();
        Ok(())
    }

    #[test]
    fn stale_duplicate_out_of_range_and_expired_replies_never_approve()
    -> Result<(), Box<dyn std::error::Error>> {
        let state = PromptState::default();
        let first = state
            .begin(PromptContent::QuitRecording, Duration::from_secs(30))
            .ok_or("begin")?;
        let generation = state.snapshot().ok_or("snapshot")?.generation;
        assert!(!state.reply(PromptReply {
            generation: generation + 1,
            choice: 1
        }));
        assert!(!state.reply(PromptReply {
            generation,
            choice: 2
        }));
        assert!(state.reply(PromptReply {
            generation,
            choice: 0
        }));
        assert!(!state.reply(PromptReply {
            generation,
            choice: 1
        }));
        assert_eq!(first.try_recv()?, 0);
        state.end();
        let expired = state
            .begin(PromptContent::QuitRecording, Duration::ZERO)
            .ok_or("begin")?;
        let next = state.snapshot().ok_or("snapshot")?.generation;
        assert!(!state.reply(PromptReply {
            generation,
            choice: 1
        }));
        assert!(!state.reply(PromptReply {
            generation: next,
            choice: 1
        }));
        state.end();
        assert!(expired.recv().is_err());
        Ok(())
    }
}
