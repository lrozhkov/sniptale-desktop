//! Active resident-pipe session read cancellation registry.

use std::sync::{Arc, Mutex};

use crate::ipc::AgentPipeReadShutdown;

#[derive(Clone, Default)]
pub(crate) struct AgentPipeSessionShutdowns {
    entries: Arc<Mutex<Vec<AgentPipeSessionShutdownEntry>>>,
}

struct AgentPipeSessionShutdownEntry {
    session_id: String,
    shutdown: Box<dyn AgentPipeReadShutdown>,
}

impl AgentPipeSessionShutdowns {
    pub(crate) fn register<S>(
        &self,
        session_id: &str,
        shutdown: &S,
    ) -> Result<AgentPipeSessionShutdownGuard, AgentPipeSessionShutdownError>
    where
        S: AgentPipeReadShutdown + Clone + 'static,
    {
        self.entries
            .lock()
            .map_err(|_| AgentPipeSessionShutdownError)?
            .push(AgentPipeSessionShutdownEntry {
                session_id: session_id.to_owned(),
                shutdown: Box::new(shutdown.clone()),
            });
        Ok(AgentPipeSessionShutdownGuard {
            session_id: session_id.to_owned(),
            shutdowns: self.clone(),
        })
    }

    pub(crate) fn cancel_all(&self) -> Result<(), AgentPipeSessionShutdownError> {
        let entries = self
            .entries
            .lock()
            .map_err(|_| AgentPipeSessionShutdownError)?;
        for entry in entries.iter() {
            entry.shutdown.shutdown_agent_read();
        }
        Ok(())
    }

    fn unregister(&self, session_id: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.retain(|entry| entry.session_id != session_id);
        }
    }
}

pub(crate) struct AgentPipeSessionShutdownGuard {
    session_id: String,
    shutdowns: AgentPipeSessionShutdowns,
}

impl Drop for AgentPipeSessionShutdownGuard {
    fn drop(&mut self) {
        self.shutdowns.unregister(&self.session_id);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AgentPipeSessionShutdownError;
