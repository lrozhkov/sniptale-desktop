//! Authenticated resident pipe session thread orchestration.

use std::{
    io::{Read, Write},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::Duration,
};

use crate::{
    app::{
        resident::AgentOutboundEvent,
        responses::{NativeHostSession, NativeHostSharedState, NativeHostSideEffects},
    },
    ipc::{AgentPipeReadShutdown, AuthenticatedPipeSession},
};

use super::{
    logging,
    outbound::run_outbound_event_forwarder,
    progress::{AuthenticatedSessionRuntime, SessionProgress, run_session_progress_watchdog},
    reader::{ForwardedPayloadRuntime, run_forwarded_payload_reader},
};
use crate::app::bridge::session::AgentPipeSessionError;

const SESSION_MODE_EPOCH_POLL_INTERVAL: Duration = Duration::from_millis(25);

pub(super) fn run_authenticated_agent_pipe_session<R, W, S>(
    mut reader: R,
    writer: W,
    authenticated: &mut AuthenticatedPipeSession,
    effects: &mut impl NativeHostSideEffects,
    events: mpsc::Receiver<AgentOutboundEvent>,
    shared_state: NativeHostSharedState,
    runtime: AuthenticatedSessionRuntime<'_, S>,
) -> Result<(), AgentPipeSessionError>
where
    R: Read + Send,
    W: Write + Send,
    S: AgentPipeReadShutdown + Clone,
{
    #[cfg(test)]
    let session = NativeHostSession::with_shared_state(shared_state);
    #[cfg(not(test))]
    let session = NativeHostSession::system_clocked_with_shared_state(shared_state);
    let session = Arc::new(Mutex::new(session));
    let writer = Arc::new(Mutex::new(writer));
    let running = Arc::new(AtomicBool::new(true));
    let writer_error = Arc::new(Mutex::new(None));
    let runtime_log = runtime.log;
    let runtime_progress: &Arc<SessionProgress> = runtime.progress;
    let runtime_shutdown = runtime.shutdown;

    let result = thread::scope(|scope| {
        let (watchdog_stop_sender, watchdog_stop_receiver) = mpsc::sync_channel(1);
        let outbound_session = Arc::clone(&session);
        let outbound_running = Arc::clone(&running);
        let outbound_writer = Arc::clone(&writer);
        let outbound_error = Arc::clone(&writer_error);
        let outbound_log = logging::clone_log_sink(runtime_log);
        let outbound_mode_epoch = runtime.mode_epoch.clone();
        scope.spawn(move || {
            run_outbound_event_forwarder(
                &events,
                &outbound_session,
                &outbound_writer,
                &outbound_running,
                &outbound_error,
                outbound_log.as_ref(),
                outbound_mode_epoch.as_ref(),
            );
        });

        if let Some(mode_epoch) = runtime.mode_epoch.clone() {
            let mode_running = Arc::clone(&running);
            let mode_shutdown = runtime_shutdown.clone();
            let mode_log = logging::clone_log_sink(runtime_log);
            scope.spawn(move || {
                while mode_running.load(Ordering::Acquire) {
                    if !mode_epoch.remains_current() {
                        logging::log_pipe_session_event(
                            mode_log.as_ref(),
                            "session-cancelled-mode-epoch",
                            "resident pipe cancelled stale extension session",
                            Some("resident_pipe.session_mode_epoch_changed"),
                        );
                        mode_shutdown.shutdown_agent_read();
                        break;
                    }
                    thread::sleep(SESSION_MODE_EPOCH_POLL_INTERVAL);
                }
            });
        }

        let watchdog_progress = Arc::clone(runtime_progress);
        let watchdog_running = Arc::clone(&running);
        let watchdog_log = logging::clone_log_sink(runtime_log);
        scope.spawn(move || {
            run_session_progress_watchdog(
                &watchdog_progress,
                &watchdog_running,
                &runtime_shutdown,
                &watchdog_stop_receiver,
                watchdog_log.as_ref(),
            );
        });

        let reader_runtime = ForwardedPayloadRuntime {
            log: runtime_log,
            progress: runtime_progress,
            mode_epoch: runtime.mode_epoch.as_ref(),
        };
        let result = run_forwarded_payload_reader(
            &mut reader,
            &writer,
            &session,
            authenticated,
            effects,
            &reader_runtime,
        );
        running.store(false, Ordering::Release);
        let _ = watchdog_stop_sender.try_send(());
        result
    });
    let released_current_controller_generation = session
        .lock()
        .ok()
        .and_then(|mut session| session.release_controller_lease());
    if let Some(generation) = released_current_controller_generation {
        let _ = effects.mark_tray_connection_disconnected(generation);
    }
    result?;
    if let Ok(mut error) = writer_error.lock()
        && let Some(error) = error.take()
    {
        return Err(error);
    }
    Ok(())
}
