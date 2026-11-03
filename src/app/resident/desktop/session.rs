//! Main-thread `WebView` loop and the existing tray worker's presentation pump.

#[cfg(not(test))]
use super::super::{
    AgentComponent, AgentEventHub, AgentRunError, AgentShutdownSignal, AgentStartupStatus,
    worker_start::WorkerHandle,
};
use super::backend::ResidentDesktopBackend;
#[cfg(not(test))]
use crate::tray::desktop::DesktopRuntime;
use crate::tray::desktop::{
    DesktopHandle,
    model::{CapturePhase, CaptureSnapshot, DesktopBackend},
};
#[cfg(not(test))]
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(crate) struct DesktopSession {
    handle: DesktopHandle,
    backend: Arc<ResidentDesktopBackend>,
    published: Arc<Mutex<Option<CaptureSnapshot>>>,
}

impl std::fmt::Debug for DesktopSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DesktopSession")
            .finish_non_exhaustive()
    }
}

impl DesktopSession {
    pub(crate) fn pump(&self, source: &mut impl crate::tray::TrayCommandSource) {
        source.attach_desktop(self.handle.clone());
        self.backend
            .observe_window_available(source.capture_window_available());
        let _ = self
            .backend
            .flush_preferences_update(|value| self.handle.publish_preferences(value));
        let Ok(snapshot) = self.backend.snapshot() else {
            return;
        };
        if matches!(
            snapshot.phase,
            CapturePhase::Recording
                | CapturePhase::Paused
                | CapturePhase::Saving
                | CapturePhase::Complete
                | CapturePhase::Failed
        ) {
            let _ = self.handle.restore_capture();
        }
        if settings_can_return(snapshot.phase) {
            let _ = self.handle.restore_settings();
        }
        if let Ok(mut published) = self.published.lock()
            && published.as_ref() != Some(&snapshot)
            && self.handle.publish_capture(&snapshot).is_ok()
        {
            *published = Some(snapshot);
        }
    }
}

fn settings_can_return(phase: CapturePhase) -> bool {
    matches!(
        phase,
        CapturePhase::Idle | CapturePhase::Saving | CapturePhase::Complete | CapturePhase::Failed
    )
}

#[cfg(test)]
mod restore_tests {
    use super::{CapturePhase, settings_can_return};

    #[test]
    fn settings_return_only_after_source_capture_has_finished() {
        for phase in [
            CapturePhase::Idle,
            CapturePhase::Saving,
            CapturePhase::Complete,
            CapturePhase::Failed,
        ] {
            assert!(settings_can_return(phase));
        }
        for phase in [
            CapturePhase::Selecting,
            CapturePhase::Starting,
            CapturePhase::Recording,
            CapturePhase::Paused,
        ] {
            assert!(!settings_can_return(phase));
        }
    }
}

#[cfg(not(test))]
pub(in crate::app::resident) fn run(
    workers: &[WorkerHandle],
    settings: &crate::settings::ActiveSettings,
    status: AgentStartupStatus,
    events: &AgentEventHub,
    shutdown: Receiver<AgentShutdownSignal>,
) -> Result<(), AgentRunError> {
    let sender = workers
        .iter()
        .find(|worker| worker.component == AgentComponent::Tray)
        .ok_or(AgentRunError::DesktopRuntimeUnavailable)?
        .command_sender
        .clone();
    let backend = Arc::new(ResidentDesktopBackend::new(
        settings.clone(),
        events.desktop.clone(),
        sender,
        status == AgentStartupStatus::Operational,
    ));
    let runtime = DesktopRuntime::build(backend.clone())
        .map_err(|_| AgentRunError::DesktopRuntimeUnavailable)?;
    let handle = runtime.handle();
    let session = DesktopSession {
        handle: handle.clone(),
        backend,
        published: Arc::new(Mutex::new(None)),
    };
    *events
        .desktop_session
        .lock()
        .map_err(|_| AgentRunError::DesktopRuntimeUnavailable)? = Some(session);
    // The tray's existing shutdown sender is authoritative. Panel closure never sends Stop.
    let watcher = std::thread::Builder::new()
        .name("desktop-shutdown".into())
        .spawn(move || {
            let _ = shutdown.recv();
            handle.stop();
        })
        .map_err(|_| AgentRunError::DesktopRuntimeUnavailable)?;
    let result = runtime
        .run()
        .map_err(|_| AgentRunError::DesktopRuntimeUnavailable);
    if let Ok(mut session) = events.desktop_session.lock() {
        *session = None;
    }
    // The sender may remain live until workers stop. Do not join it from the event loop.
    drop(watcher);
    result
}
