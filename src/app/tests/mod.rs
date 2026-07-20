use std::{
    cell::RefCell,
    collections::VecDeque,
    error::Error,
    ffi::OsString,
    fs,
    io::{Cursor, Error as IoError, ErrorKind, Read, Write},
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use serde::Serialize;
use serde_json::Value;

use super::bridge::session::{
    AgentPipeSessionConfig, AgentPipeSessionError, AgentPipeSplit, run_agent_pipe_session,
    run_agent_pipe_session_with_effects, run_agent_pipe_session_with_effects_and_events,
    run_agent_pipe_session_with_effects_events_and_state,
};
use super::bridge::{
    BridgeStartupError, BridgeStartupInputs, NativeHostPipeAuthConfig,
    connect_pipe_with_bounded_agent_startup, connect_pipe_with_typed_agent_startup,
    plan_bridge_startup, run_authenticated_pipe_forwarding_bridge,
};
use super::host::{run_with_io, run_with_streams};
use super::resident::pipe_broker::{
    AgentPipeAccept, AgentPipeClock, AgentPipeDuplexListener, AgentPipeEntropy, AgentPipeListener,
    AgentPipeRejection, AgentPipeServerControl, AgentPipeServerError, AgentPipeServerReport,
    AgentPipeSessionConfigError, AgentPipeSessionConfigGenerator, AgentPipeSessionConfigSource,
    run_agent_pipe_server, run_agent_pipe_server_with_effects,
    run_agent_pipe_server_with_effects_and_events,
};
use super::resident::shutdown::{ActiveWork, ShutdownInputs, ShutdownStep, shutdown_sequence};
use super::resident::{
    AgentComponent, AgentEventHub, AgentEventPublishResult, AgentRunInputs, AgentShutdownSignal,
    AgentStartupInputs, AgentStartupStatus, plan_agent_start, run_resident_agent_until_shutdown,
};
use super::responses::{CapabilityFacts, app_hello, capabilities_from_facts, install_state};
use super::responses::{
    NativeHostCommandEvent, NativeHostCommandFailureEvent, NativeHostCommandOutcome,
    NativeHostSession, NativeHostSharedState, NativeHostSideEffectError, NativeHostSideEffects,
    responses_for_command_event, responses_for_command_failure, responses_for_message,
    responses_for_message_with_effects,
};
use super::responses::{NativeHostTransferChunk, NativeHostTransferChunkError};
use super::responses::{responses_for_synced_tray_action, responses_for_tray_action};
use super::support::{recording_doctor_detail, recording_doctor_status};
use crate::{
    audio::{AudioCapabilitySummary, MicrophoneDeviceDescriptor},
    capture::{PendingScreenshotTransfer, RegionOverlayAvailability, WgcRuntimeAvailability},
    diagnostics::DoctorStatus,
    hotkeys::HotkeyRegistrationPlan,
    ipc::{
        AgentPipeReadShutdown, BridgeHello, PipeChallenge, PipeSecret, PipeServerAuth,
        agent_pipe_response_header_payload, build_auth_response,
    },
    protocol::{
        AppRecordingProgressMessage, AppScreenshotStartMessage, ContainerCodec,
        OperationFailedOperation, OperationalErrorKind, RecordingProgressStatus,
        ScreenshotMimeType, ScreenshotMode, VideoCodec, VideoMode, WireMessage, parse_wire_message,
    },
    recording::{
        MediaFoundationAvailability, PendingRecordingTransfer, RecordingRuntimeAvailability,
        RecordingStartPolicy, VideoEncoderAvailability,
    },
    transfer::chunks::EncodedChunk,
    tray::{TrayActionKind, TrayActionRequest, TrayNoticeKind},
};

const ALLOWED_NATIVE_ORIGIN: &str = "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/";

fn args(items: &[&str]) -> Vec<OsString> {
    items.iter().map(OsString::from).collect()
}

mod command_flow;
mod host_modes;
mod native_session;
mod resident_bridge;
mod startup;
mod tray_bridge;

fn pipe_server_session_input(
    session_id: &str,
    server_nonce: &str,
    forwarded_payload: &[u8],
) -> Result<Vec<u8>, Box<dyn Error>> {
    pipe_server_session_input_many(session_id, server_nonce, &[forwarded_payload])
}

fn pipe_server_hello_only_input() -> Result<Vec<u8>, Box<dyn Error>> {
    frame_json(&bridge_hello())
}

fn pipe_server_session_input_many(
    session_id: &str,
    server_nonce: &str,
    forwarded_payloads: &[&[u8]],
) -> Result<Vec<u8>, Box<dyn Error>> {
    let challenge = pipe_challenge_with(session_id, server_nonce);
    let response = build_auth_response(&challenge, &pipe_secret()?)?;
    let mut input = frame_json(&bridge_hello())?;
    input.extend(frame_json(&response)?);
    for payload in forwarded_payloads {
        input.extend(framed(payload)?);
    }
    Ok(input)
}

fn connected_native_host_session() -> Result<NativeHostSession, Box<dyn Error>> {
    let mut session = NativeHostSession::default();
    for payload in [extension_hello(), extension_controller_acquire()] {
        let message = parse_wire_message(payload)?;
        let value: Value = serde_json::from_slice(payload)?;
        responses_for_message(&mut session, &message, &value);
    }
    Ok(session)
}

fn settings_synced_native_host_session() -> Result<NativeHostSession, Box<dyn Error>> {
    let mut session = NativeHostSession::default();
    sync_native_host_session(&mut session)?;
    Ok(session)
}

fn settings_synced_native_host_session_with_shared_state(
    shared_state: NativeHostSharedState,
) -> Result<NativeHostSession, Box<dyn Error>> {
    let mut session = NativeHostSession::with_shared_state(shared_state);
    sync_native_host_session(&mut session)?;
    Ok(session)
}

fn sync_native_host_session(session: &mut NativeHostSession) -> Result<(), Box<dyn Error>> {
    for payload in [
        extension_hello(),
        extension_controller_acquire(),
        extension_settings_sync(),
    ] {
        let message = parse_wire_message(payload)?;
        let value: Value = serde_json::from_slice(payload)?;
        responses_for_message(&mut *session, &message, &value);
    }
    Ok(())
}

fn tray_request(action_id: &str, invocation_id: &str, kind: TrayActionKind) -> TrayActionRequest {
    TrayActionRequest {
        action_id: action_id.to_owned(),
        invocation_id: invocation_id.to_owned(),
        requested_at_epoch_ms: 4_000,
        kind,
    }
}

fn framed(payload: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let length = u32::try_from(payload.len())?;
    let mut frame = length.to_le_bytes().to_vec();
    frame.extend_from_slice(payload);
    Ok(frame)
}

fn frame_json(value: &impl Serialize) -> Result<Vec<u8>, Box<dyn Error>> {
    framed(&serde_json::to_vec(value)?)
}

fn framed_agent_responses(responses: &[&[u8]]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut output = framed(&agent_pipe_response_header_payload(responses.len())?)?;
    for response in responses {
        output.extend(framed(response)?);
    }
    Ok(output)
}

fn pipe_auth(max_sessions: usize, queue_capacity: usize) -> Result<PipeServerAuth, Box<dyn Error>> {
    Ok(PipeServerAuth::new(
        "S-1-5-21-1000",
        "install-stable",
        &[ALLOWED_NATIVE_ORIGIN],
        Some(pipe_secret()?),
        max_sessions,
        queue_capacity,
    )?)
}

fn pipe_secret() -> Result<PipeSecret, Box<dyn Error>> {
    Ok(PipeSecret::new("per-install-secret")?)
}

fn bridge_hello() -> BridgeHello {
    BridgeHello {
        current_user_sid: "S-1-5-21-1000".to_owned(),
        install_id: "install-stable".to_owned(),
        extension_origin: ALLOWED_NATIVE_ORIGIN.to_owned(),
        client_nonce: "client-nonce-0001".to_owned(),
    }
}

fn pipe_challenge() -> PipeChallenge {
    pipe_challenge_with("session-1", "server-nonce-0001")
}

fn pipe_challenge_with(session_id: &str, server_nonce: &str) -> PipeChallenge {
    PipeChallenge {
        algorithm: "pipe-auth-v2-hmac-sha256".to_owned(),
        session_id: session_id.to_owned(),
        install_id: "install-stable".to_owned(),
        client_nonce: "client-nonce-0001".to_owned(),
        server_nonce: server_nonce.to_owned(),
        expires_at_ms: 6_000,
    }
}

fn pipe_session_config() -> AgentPipeSessionConfig {
    pipe_session_config_with("session-1", "server-nonce-0001")
}

fn pipe_session_config_with(session_id: &str, server_nonce: &str) -> AgentPipeSessionConfig {
    AgentPipeSessionConfig {
        now_ms: 1_000,
        challenge_ttl_ms: 5_000,
        session_id: session_id.to_owned(),
        server_nonce: server_nonce.to_owned(),
    }
}

fn run_split_pipe_session_with_state(
    session_id: &str,
    server_nonce: &str,
    controller_acquire: impl AsRef<[u8]>,
    shared_state: NativeHostSharedState,
    auth: &mut PipeServerAuth,
    events: &AgentEventHub,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let challenge = pipe_challenge_with(session_id, server_nonce);
    let response = build_auth_response(&challenge, &pipe_secret()?)?;
    let mut input = frame_json(&bridge_hello())?;
    input.extend(frame_json(&response)?);
    input.extend(framed(extension_hello())?);
    input.extend(framed(controller_acquire.as_ref())?);

    let (release_eof, wait_for_eof) = mpsc::sync_channel(1);
    let written = Arc::new(Mutex::new(Vec::new()));
    let pipe = ThreadedSplitPipe::new(input, Arc::clone(&written), wait_for_eof);
    let mut effects = AcceptingHotkeyEffects;
    let event_receiver = events.subscribe();
    let config = pipe_session_config_with(session_id, server_nonce);

    let _ = release_eof.send(());
    run_agent_pipe_session_with_effects_events_and_state(
        pipe,
        auth,
        &config,
        &mut effects,
        event_receiver,
        shared_state,
    )?;
    Ok(written.lock().map_err(|_| IoError::other("lock"))?.clone())
}

struct DuplexPipe {
    incoming: Cursor<Vec<u8>>,
    written: Vec<u8>,
}

impl DuplexPipe {
    fn new(incoming: Vec<u8>) -> Self {
        Self {
            incoming: Cursor::new(incoming),
            written: Vec::new(),
        }
    }
}

impl Read for DuplexPipe {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.incoming.read(buffer)
    }
}

impl Write for DuplexPipe {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.written.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct ThreadedSplitPipe {
    incoming: Option<Cursor<Vec<u8>>>,
    written: Arc<Mutex<Vec<u8>>>,
    wait_for_eof: Option<mpsc::Receiver<()>>,
    wait_for_split: Option<mpsc::Receiver<()>>,
    shutdown_sender: Option<mpsc::SyncSender<()>>,
    read_error_after_eof: Option<ErrorKind>,
    write_error: Option<ErrorKind>,
    writes_before_error: usize,
}

impl ThreadedSplitPipe {
    fn new(
        incoming: Vec<u8>,
        written: Arc<Mutex<Vec<u8>>>,
        wait_for_eof: mpsc::Receiver<()>,
    ) -> Self {
        Self {
            incoming: Some(Cursor::new(incoming)),
            written,
            wait_for_eof: Some(wait_for_eof),
            wait_for_split: None,
            shutdown_sender: None,
            read_error_after_eof: None,
            write_error: None,
            writes_before_error: 0,
        }
    }

    fn new_failing_reader(
        incoming: Vec<u8>,
        written: Arc<Mutex<Vec<u8>>>,
        wait_for_eof: mpsc::Receiver<()>,
        read_error_after_eof: ErrorKind,
    ) -> Self {
        Self {
            incoming: Some(Cursor::new(incoming)),
            written,
            wait_for_eof: Some(wait_for_eof),
            wait_for_split: None,
            shutdown_sender: None,
            read_error_after_eof: Some(read_error_after_eof),
            write_error: None,
            writes_before_error: 0,
        }
    }

    fn new_failing_writer_after_writes(
        incoming: Vec<u8>,
        written: Arc<Mutex<Vec<u8>>>,
        wait_for_eof: mpsc::Receiver<()>,
        write_error: ErrorKind,
        writes_before_error: usize,
    ) -> Self {
        Self {
            incoming: Some(Cursor::new(incoming)),
            written,
            wait_for_eof: Some(wait_for_eof),
            wait_for_split: None,
            shutdown_sender: None,
            read_error_after_eof: None,
            write_error: Some(write_error),
            writes_before_error,
        }
    }

    fn with_shutdown_sender(mut self, shutdown_sender: mpsc::SyncSender<()>) -> Self {
        self.shutdown_sender = Some(shutdown_sender);
        self
    }

    fn with_split_gate(mut self, wait_for_split: mpsc::Receiver<()>) -> Self {
        self.wait_for_split = Some(wait_for_split);
        self
    }
}

impl Read for ThreadedSplitPipe {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.incoming
            .as_mut()
            .ok_or_else(|| IoError::other("split pipe reader missing"))?
            .read(buffer)
    }
}

impl Write for ThreadedSplitPipe {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.written
            .lock()
            .map_err(|_| IoError::other("split pipe writer poisoned"))?
            .extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl AgentPipeSplit for ThreadedSplitPipe {
    type Reader = ThreadedSplitPipeReader;
    type Writer = ThreadedSplitPipeWriter;
    type Shutdown = ThreadedSplitPipeShutdown;

    fn split_for_session(
        mut self,
    ) -> std::io::Result<(Self::Reader, Self::Writer, Self::Shutdown)> {
        if let Some(wait_for_split) = self.wait_for_split.take() {
            let _ = wait_for_split.recv();
        }
        Ok((
            ThreadedSplitPipeReader {
                incoming: self
                    .incoming
                    .take()
                    .ok_or_else(|| IoError::other("split pipe reader missing"))?,
                wait_for_eof: self
                    .wait_for_eof
                    .take()
                    .ok_or_else(|| IoError::other("split pipe eof gate missing"))?,
                read_error_after_eof: self.read_error_after_eof,
            },
            ThreadedSplitPipeWriter {
                written: self.written,
                write_error: self.write_error,
                writes_before_error: self.writes_before_error,
            },
            ThreadedSplitPipeShutdown {
                sender: self.shutdown_sender,
            },
        ))
    }
}

#[derive(Clone)]
struct ThreadedSplitPipeShutdown {
    sender: Option<mpsc::SyncSender<()>>,
}

impl AgentPipeReadShutdown for ThreadedSplitPipeShutdown {
    fn shutdown_agent_read(&self) {
        if let Some(sender) = &self.sender {
            let _ = sender.try_send(());
        }
    }
}

struct ThreadedPipeListener {
    pipes: VecDeque<ThreadedSplitPipe>,
}

impl ThreadedPipeListener {
    fn new(pipes: Vec<ThreadedSplitPipe>) -> Self {
        Self {
            pipes: pipes.into(),
        }
    }
}

impl AgentPipeDuplexListener for ThreadedPipeListener {
    type Pipe = ThreadedSplitPipe;

    fn accept(&mut self) -> Result<AgentPipeAccept<Self::Pipe>, AgentPipeServerError> {
        Ok(self
            .pipes
            .pop_front()
            .map_or(AgentPipeAccept::Closed, AgentPipeAccept::Connected))
    }
}

struct ThreadedSplitPipeReader {
    incoming: Cursor<Vec<u8>>,
    wait_for_eof: mpsc::Receiver<()>,
    read_error_after_eof: Option<ErrorKind>,
}

impl Read for ThreadedSplitPipeReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.incoming.read(buffer)?;
        if read == 0 {
            let _ = self.wait_for_eof.recv();
            if let Some(kind) = self.read_error_after_eof {
                return Err(IoError::new(kind, "simulated closed transport"));
            }
        }
        Ok(read)
    }
}

struct ThreadedSplitPipeWriter {
    written: Arc<Mutex<Vec<u8>>>,
    write_error: Option<ErrorKind>,
    writes_before_error: usize,
}

impl Write for ThreadedSplitPipeWriter {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        if let Some(kind) = self.write_error {
            if self.writes_before_error == 0 {
                return Err(IoError::new(kind, "simulated closed transport"));
            }
            self.writes_before_error -= 1;
        }
        self.written
            .lock()
            .map_err(|_| IoError::other("split pipe writer poisoned"))?
            .extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

struct VecPipeListener {
    pipes: VecDeque<SharedDuplexPipe>,
}

impl VecPipeListener {
    fn new(pipes: Vec<SharedDuplexPipe>) -> Self {
        Self {
            pipes: pipes.into(),
        }
    }

    fn remaining(&self) -> usize {
        self.pipes.len()
    }
}

impl AgentPipeListener for VecPipeListener {
    type Pipe = SharedDuplexPipe;

    fn accept(&mut self) -> Result<AgentPipeAccept<Self::Pipe>, AgentPipeServerError> {
        Ok(self
            .pipes
            .pop_front()
            .map_or(AgentPipeAccept::Closed, AgentPipeAccept::Connected))
    }
}

enum ScriptedPipeAccept {
    Connected(SharedDuplexPipe),
    Pending,
    Rejected(AgentPipeRejection),
}

struct ScriptedPipeListener {
    accepts: VecDeque<ScriptedPipeAccept>,
}

impl ScriptedPipeListener {
    fn new(accepts: Vec<ScriptedPipeAccept>) -> Self {
        Self {
            accepts: accepts.into(),
        }
    }

    fn remaining(&self) -> usize {
        self.accepts.len()
    }
}

impl AgentPipeListener for ScriptedPipeListener {
    type Pipe = SharedDuplexPipe;

    fn accept(&mut self) -> Result<AgentPipeAccept<Self::Pipe>, AgentPipeServerError> {
        Ok(match self.accepts.pop_front() {
            Some(ScriptedPipeAccept::Connected(pipe)) => AgentPipeAccept::Connected(pipe),
            Some(ScriptedPipeAccept::Pending) => AgentPipeAccept::Pending,
            Some(ScriptedPipeAccept::Rejected(rejection)) => AgentPipeAccept::Rejected(rejection),
            None => AgentPipeAccept::Closed,
        })
    }
}

struct FixedSessionConfigs {
    configs: VecDeque<AgentPipeSessionConfig>,
}

impl FixedSessionConfigs {
    fn new(configs: Vec<AgentPipeSessionConfig>) -> Self {
        Self {
            configs: configs.into(),
        }
    }
}

impl AgentPipeSessionConfigSource for FixedSessionConfigs {
    fn next_session_config(
        &mut self,
    ) -> Result<AgentPipeSessionConfig, AgentPipeSessionConfigError> {
        self.configs
            .pop_front()
            .ok_or(AgentPipeSessionConfigError::Entropy)
    }
}

struct FixedEntropy {
    values: VecDeque<[u8; 16]>,
}

impl FixedEntropy {
    fn new(values: impl IntoIterator<Item = [u8; 16]>) -> Self {
        Self {
            values: values.into_iter().collect(),
        }
    }
}

impl AgentPipeEntropy for FixedEntropy {
    fn next_bytes(&mut self) -> Result<[u8; 16], AgentPipeSessionConfigError> {
        self.values
            .pop_front()
            .ok_or(AgentPipeSessionConfigError::Entropy)
    }
}

struct FixedClock {
    values: VecDeque<u64>,
}

impl FixedClock {
    fn new(values: impl IntoIterator<Item = u64>) -> Self {
        Self {
            values: values.into_iter().collect(),
        }
    }
}

impl AgentPipeClock for FixedClock {
    fn now_ms(&mut self) -> u64 {
        self.values.pop_front().unwrap_or(0)
    }
}

struct FailingHotkeyEffects;

impl NativeHostSideEffects for FailingHotkeyEffects {
    fn apply_hotkey_plan(
        &mut self,
        _plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        Err(NativeHostSideEffectError::HotkeyApply)
    }
}

struct AcceptingHotkeyEffects;

impl NativeHostSideEffects for AcceptingHotkeyEffects {
    fn apply_hotkey_plan(
        &mut self,
        _plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        Ok(())
    }
}

#[derive(Default)]
struct ConnectionProjectionEffects {
    disconnects: u8,
    connected_generation: Option<u64>,
}

impl NativeHostSideEffects for ConnectionProjectionEffects {
    fn apply_hotkey_plan(
        &mut self,
        _plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        Ok(())
    }

    fn mark_tray_connection_connected(
        &mut self,
        generation: u64,
    ) -> Result<(), NativeHostSideEffectError> {
        self.connected_generation = Some(generation);
        Ok(())
    }

    fn mark_tray_connection_disconnected(
        &mut self,
        generation: u64,
    ) -> Result<(), NativeHostSideEffectError> {
        assert_eq!(self.connected_generation, Some(generation));
        self.disconnects = self.disconnects.saturating_add(1);
        Ok(())
    }
}

struct PendingRecordingEffects {
    pending: Vec<PendingRecordingTransfer>,
}

impl NativeHostSideEffects for PendingRecordingEffects {
    fn apply_hotkey_plan(
        &mut self,
        _plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        Ok(())
    }

    fn pending_recordings_for_reconnect(
        &mut self,
    ) -> Result<Vec<PendingRecordingTransfer>, NativeHostSideEffectError> {
        Ok(std::mem::take(&mut self.pending))
    }
}

struct PendingScreenshotEffects {
    pending: Vec<PendingScreenshotTransfer>,
}

impl NativeHostSideEffects for PendingScreenshotEffects {
    fn apply_hotkey_plan(
        &mut self,
        _plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        Ok(())
    }

    fn pending_screenshots_for_reconnect(
        &mut self,
    ) -> Result<Vec<PendingScreenshotTransfer>, NativeHostSideEffectError> {
        Ok(std::mem::take(&mut self.pending))
    }
}

#[derive(Default)]
struct TrayNoticeEffects {
    notices: Vec<TrayNoticeKind>,
}

impl NativeHostSideEffects for TrayNoticeEffects {
    fn apply_hotkey_plan(
        &mut self,
        _plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        Ok(())
    }

    fn show_tray_notice(&mut self, kind: TrayNoticeKind) -> Result<(), NativeHostSideEffectError> {
        self.notices.push(kind);
        Ok(())
    }
}

struct FailingReconnectEffects;

impl NativeHostSideEffects for FailingReconnectEffects {
    fn apply_hotkey_plan(
        &mut self,
        _plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        Ok(())
    }

    fn pending_recordings_for_reconnect(
        &mut self,
    ) -> Result<Vec<PendingRecordingTransfer>, NativeHostSideEffectError> {
        Err(NativeHostSideEffectError::RecordingRecovery)
    }
}

struct CommandDispatchEffects {
    screenshot_outcome: Result<NativeHostCommandOutcome, NativeHostSideEffectError>,
    recording_outcome: Result<NativeHostCommandOutcome, NativeHostSideEffectError>,
    screenshot_starts: u32,
    recording_starts: u32,
    last_recording_policy: Option<RecordingStartPolicy>,
}

impl Default for CommandDispatchEffects {
    fn default() -> Self {
        Self {
            screenshot_outcome: Ok(NativeHostCommandOutcome::UnsupportedCapability),
            recording_outcome: Ok(NativeHostCommandOutcome::UnsupportedCapability),
            screenshot_starts: 0,
            recording_starts: 0,
            last_recording_policy: None,
        }
    }
}

impl NativeHostSideEffects for CommandDispatchEffects {
    fn apply_hotkey_plan(
        &mut self,
        _plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        Ok(())
    }

    fn start_screenshot_capture(
        &mut self,
        command: &crate::protocol::ExtensionScreenshotCaptureCommandMessage,
    ) -> Result<NativeHostCommandOutcome, NativeHostSideEffectError> {
        assert_eq!(command.command_id, "command-1");
        self.screenshot_starts += 1;
        self.screenshot_outcome.clone()
    }

    fn start_recording(
        &mut self,
        command: &crate::protocol::ExtensionRecordingStartCommandMessage,
        policy: RecordingStartPolicy,
    ) -> Result<NativeHostCommandOutcome, NativeHostSideEffectError> {
        assert_eq!(command.command_id, "command-1");
        self.recording_starts += 1;
        self.last_recording_policy = Some(policy);
        self.recording_outcome.clone()
    }
}

struct TransferChunkEffects {
    screenshot_results:
        VecDeque<Result<Option<NativeHostTransferChunk>, NativeHostTransferChunkError>>,
    recording_results: VecDeque<Result<Option<EncodedChunk>, NativeHostTransferChunkError>>,
    screenshot_requests: Vec<(String, u32, u64)>,
    recording_requests: Vec<(String, u32, u64)>,
}

impl Default for TransferChunkEffects {
    fn default() -> Self {
        Self {
            screenshot_results: VecDeque::from([Ok(None)]),
            recording_results: VecDeque::from([Ok(None)]),
            screenshot_requests: Vec::new(),
            recording_requests: Vec::new(),
        }
    }
}

impl NativeHostSideEffects for TransferChunkEffects {
    fn apply_hotkey_plan(
        &mut self,
        _plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        Ok(())
    }

    fn screenshot_transfer_chunk(
        &mut self,
        capture_id: &str,
        chunk_index: u32,
        now_ms: u64,
    ) -> Result<Option<NativeHostTransferChunk>, NativeHostTransferChunkError> {
        self.screenshot_requests
            .push((capture_id.to_owned(), chunk_index, now_ms));
        self.screenshot_results.pop_front().unwrap_or(Ok(None))
    }

    fn recording_transfer_chunk(
        &mut self,
        recording_id: &str,
        chunk_index: u32,
        now_ms: u64,
    ) -> Result<Option<EncodedChunk>, NativeHostTransferChunkError> {
        self.recording_requests
            .push((recording_id.to_owned(), chunk_index, now_ms));
        self.recording_results.pop_front().unwrap_or(Ok(None))
    }
}

struct RecordingTerminalEffects {
    ack_result: Result<bool, NativeHostSideEffectError>,
    reject_result: Result<bool, NativeHostSideEffectError>,
    acked_recording_id: Option<String>,
    rejected_recording_id: Option<String>,
    retain_for_diagnostics: Option<bool>,
}

impl Default for RecordingTerminalEffects {
    fn default() -> Self {
        Self {
            ack_result: Ok(false),
            reject_result: Ok(false),
            acked_recording_id: None,
            rejected_recording_id: None,
            retain_for_diagnostics: None,
        }
    }
}

impl NativeHostSideEffects for RecordingTerminalEffects {
    fn apply_hotkey_plan(
        &mut self,
        _plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        Ok(())
    }

    fn ack_recording_transfer(
        &mut self,
        recording_id: &str,
    ) -> Result<bool, NativeHostSideEffectError> {
        self.acked_recording_id = Some(recording_id.to_owned());
        self.ack_result
    }

    fn reject_recording_transfer(
        &mut self,
        recording_id: &str,
        retain_for_diagnostics: bool,
    ) -> Result<bool, NativeHostSideEffectError> {
        self.rejected_recording_id = Some(recording_id.to_owned());
        self.retain_for_diagnostics = Some(retain_for_diagnostics);
        self.reject_result
    }
}

struct ScreenshotTerminalEffects {
    ack_result: Result<bool, NativeHostSideEffectError>,
    reject_result: Result<bool, NativeHostSideEffectError>,
    acked_capture_id: Option<String>,
    rejected_capture_id: Option<String>,
    retain_for_diagnostics: Option<bool>,
}

impl Default for ScreenshotTerminalEffects {
    fn default() -> Self {
        Self {
            ack_result: Ok(false),
            reject_result: Ok(false),
            acked_capture_id: None,
            rejected_capture_id: None,
            retain_for_diagnostics: None,
        }
    }
}

impl NativeHostSideEffects for ScreenshotTerminalEffects {
    fn apply_hotkey_plan(
        &mut self,
        _plan: HotkeyRegistrationPlan,
    ) -> Result<(), NativeHostSideEffectError> {
        Ok(())
    }

    fn ack_screenshot_transfer(
        &mut self,
        capture_id: &str,
    ) -> Result<bool, NativeHostSideEffectError> {
        self.acked_capture_id = Some(capture_id.to_owned());
        self.ack_result
    }

    fn reject_screenshot_transfer(
        &mut self,
        capture_id: &str,
        retain_for_diagnostics: bool,
    ) -> Result<bool, NativeHostSideEffectError> {
        self.rejected_capture_id = Some(capture_id.to_owned());
        self.retain_for_diagnostics = Some(retain_for_diagnostics);
        self.reject_result
    }
}

fn encoded_test_chunk(index: u64) -> EncodedChunk {
    EncodedChunk {
        index,
        byte_offset: index.saturating_mul(crate::protocol::CHUNK_RAW_BYTES),
        raw_bytes: 11,
        chunk_sha256: "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9".to_owned(),
        base64: "aGVsbG8gd29ybGQ=".to_owned(),
    }
}

fn native_test_transfer_chunk(index: u64, is_complete: bool) -> NativeHostTransferChunk {
    NativeHostTransferChunk {
        chunk: encoded_test_chunk(index),
        is_complete,
    }
}

fn pending_recording_transfer() -> PendingRecordingTransfer {
    PendingRecordingTransfer {
        recording_id: "recording-1".to_owned(),
        filename: "sniptale-recording.mp4".to_owned(),
        duration_ms: 4_000,
        total_bytes: crate::protocol::CHUNK_RAW_BYTES + 1,
        width: 1920,
        height: 1080,
        fps: 30,
        sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        chunk_count: 2,
        next_missing_chunk_index: 0,
        open_editor: true,
    }
}

fn pending_screenshot_transfer() -> PendingScreenshotTransfer {
    PendingScreenshotTransfer {
        capture_id: "capture-1".to_owned(),
        mode: ScreenshotMode::Screen,
        filename: "capture-1.png".to_owned(),
        total_bytes: crate::protocol::CHUNK_RAW_BYTES + 1,
        width: 1920,
        height: 1080,
        sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        chunk_count: 2,
        open_editor: true,
        captured_at_epoch_ms: 2_000,
    }
}

struct SharedDuplexPipe {
    incoming: Cursor<Vec<u8>>,
    written: Rc<RefCell<Vec<u8>>>,
}

impl SharedDuplexPipe {
    fn new(incoming: Vec<u8>, written: Rc<RefCell<Vec<u8>>>) -> Self {
        Self {
            incoming: Cursor::new(incoming),
            written,
        }
    }
}

impl Read for SharedDuplexPipe {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.incoming.read(buffer)
    }
}

impl Write for SharedDuplexPipe {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.written.borrow_mut().extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn read_framed_json(output: &[u8]) -> Result<Value, Box<dyn Error>> {
    let values = read_framed_json_values(output)?;
    values
        .into_iter()
        .next()
        .ok_or_else(|| IoError::new(ErrorKind::UnexpectedEof, "missing frame").into())
}

fn wait_for_written_type(
    written: &Arc<Mutex<Vec<u8>>>,
    message_type: &str,
) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let contains_type = {
            let output = written.lock().map_err(|_| IoError::other("lock"))?;
            read_framed_json_values(&output).is_ok_and(|values| {
                values
                    .iter()
                    .any(|value| value["type"].as_str() == Some(message_type))
            })
        };
        if contains_type {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(10));
    }
    Err(IoError::new(
        ErrorKind::TimedOut,
        format!("timed out waiting for {message_type}"),
    )
    .into())
}

fn read_framed_json_values(output: &[u8]) -> Result<Vec<Value>, Box<dyn Error>> {
    let mut values = Vec::new();
    let mut offset = 0;
    while offset < output.len() {
        let header: [u8; 4] = output
            .get(offset..offset + 4)
            .ok_or_else(|| IoError::new(ErrorKind::UnexpectedEof, "missing frame header"))?
            .try_into()?;
        offset += 4;
        let length = u32::from_le_bytes(header) as usize;
        let payload = output
            .get(offset..offset + length)
            .ok_or_else(|| IoError::new(ErrorKind::UnexpectedEof, "missing frame body"))?;
        offset += length;
        values.push(serde_json::from_slice(payload)?);
    }
    Ok(values)
}

fn extension_hello() -> &'static [u8] {
    br#"{
        "type": "extension.hello",
        "protocolVersion": 1,
        "settingsSchemaVersion": 1,
        "extensionVersion": "0.1.0",
        "extensionId": "abcdefghijklmnopqrstuvwxzyabcdef",
        "minAppVersion": "0.1.0",
        "supportedProtocolVersions": [1],
        "supportedSettingsSchemaVersions": [1]
    }"#
}

fn extension_controller_acquire() -> &'static [u8] {
    br#"{
        "type": "extension.controller.acquire",
        "protocolVersion": 1,
        "connectionId": "connection-1",
        "browserFamily": "chrome",
        "profileKey": "profile-1",
        "documentId": "document-1",
        "requestedAtEpochMs": 1000,
        "reason": "initial-connect"
    }"#
}

fn extension_controller_acquire_with_connection(connection_id: &str) -> Vec<u8> {
    format!(
        r#"{{
        "type": "extension.controller.acquire",
        "protocolVersion": 1,
        "connectionId": "{connection_id}",
        "browserFamily": "chrome",
        "profileKey": "profile-{connection_id}",
        "documentId": "document-{connection_id}",
        "requestedAtEpochMs": 1000,
        "reason": "initial-connect"
    }}"#
    )
    .into_bytes()
}

fn extension_screenshot_capture_command() -> &'static [u8] {
    br#"{
        "type": "extension.screenshot.capture",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "commandId": "command-1",
        "mode": "screen",
        "settingsRevision": "settings-revision-1",
        "openEditor": true,
        "requestedAtEpochMs": 4000
    }"#
}

fn extension_recording_start_command() -> &'static [u8] {
    br#"{
        "type": "extension.recording.start",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "commandId": "command-1",
        "source": {
            "mode": "screen"
        },
        "settingsRevision": "settings-revision-1",
        "requestedQuality": {
            "audioBitrateKbps": 160,
            "audioSourceMode": "mixed",
            "frameRate": "auto",
            "quality": "HIGH",
            "videoBitrateMbpsOverride": null
        },
        "openEditor": true,
        "requestedAtEpochMs": 4000
    }"#
}

fn app_operation_failed() -> &'static [u8] {
    br#"{
        "type": "app.operation.failed",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "operation": "screenshot",
        "operationId": "command-1",
        "phase": "capture",
        "error": {
            "code": "unsupported-capability",
            "message": "native capture pipeline is unavailable",
            "recoverable": false
        },
        "occurredAtEpochMs": 1
    }"#
}

fn extension_settings_sync() -> &'static [u8] {
    br#"{
        "type": "extension.settings.sync",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "revision": "settings-revision-1",
        "schemaVersion": 1,
        "generatedAtEpochMs": 2000,
        "settings": {
            "screenshots": {
                "includeCursor": true
            },
            "video": {
                "enabled": true,
                "advanced": {
                    "audioBitrateKbps": 160,
                    "audioSourceMode": "mixed",
                    "frameRate": 30,
                    "includeCursorInVideo": true,
                    "maxDurationMinutes": 240,
                    "preferHardwareEncoder": true,
                    "videoBitrateMbpsOverride": null
                },
                "codec": {
                    "audioCodec": "aac",
                    "container": "mp4",
                    "hardwareAcceleration": "prefer",
                    "videoCodec": "h264"
                },
                "telemetry": {
                    "collectCursor": true,
                    "collectClicks": true,
                    "collectKeyEvents": true,
                    "collectTypingSpans": true,
                    "collectStaticSignals": true
                }
            },
            "trayActions": {
                "actions": [],
                "revision": "tray-revision-1"
            }
        }
    }"#
}

fn extension_settings_sync_with_tray_action() -> &'static [u8] {
    br#"{
        "type": "extension.settings.sync",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "revision": "settings-revision-2",
        "schemaVersion": 1,
        "generatedAtEpochMs": 3000,
        "settings": {
            "screenshots": {
                "includeCursor": true
            },
            "video": {
                "enabled": true,
                "advanced": {
                    "audioBitrateKbps": 160,
                    "audioSourceMode": "mixed",
                    "frameRate": 30,
                    "includeCursorInVideo": true,
                    "maxDurationMinutes": 240,
                    "preferHardwareEncoder": true,
                    "videoBitrateMbpsOverride": null
                },
                "codec": {
                    "audioCodec": "aac",
                    "container": "mp4",
                    "hardwareAcceleration": "prefer",
                    "videoCodec": "h264"
                },
                "telemetry": {
                    "collectCursor": true,
                    "collectClicks": true,
                    "collectKeyEvents": true,
                    "collectTypingSpans": true,
                    "collectStaticSignals": true
                }
            },
            "trayActions": {
                "actions": [{
                    "enabled": true,
                    "id": "screenshot-screen",
                    "kind": "capture-screenshot",
                    "label": "Capture",
                    "offlineCapable": false
                }],
                "revision": "tray-revision-2"
            }
        }
    }"#
}

fn extension_settings_sync_with_duplicate_tray_action() -> &'static [u8] {
    br#"{
        "type": "extension.settings.sync",
        "protocolVersion": 1,
        "controllerLeaseId": "lease:connection-1",
        "revision": "settings-revision-3",
        "schemaVersion": 1,
        "generatedAtEpochMs": 4000,
        "settings": {
            "screenshots": {
                "includeCursor": true
            },
            "video": {
                "enabled": true,
                "advanced": {
                    "audioBitrateKbps": 160,
                    "audioSourceMode": "mixed",
                    "frameRate": 30,
                    "includeCursorInVideo": true,
                    "maxDurationMinutes": 240,
                    "preferHardwareEncoder": true,
                    "videoBitrateMbpsOverride": null
                },
                "codec": {
                    "audioCodec": "aac",
                    "container": "mp4",
                    "hardwareAcceleration": "prefer",
                    "videoCodec": "h264"
                },
                "telemetry": {
                    "collectCursor": true,
                    "collectClicks": true,
                    "collectKeyEvents": true,
                    "collectTypingSpans": true,
                    "collectStaticSignals": true
                }
            },
            "trayActions": {
                "actions": [{
                    "enabled": true,
                    "id": "screenshot-screen",
                    "kind": "capture-screenshot",
                    "label": "Capture"
                }, {
                    "enabled": true,
                    "id": "screenshot-screen",
                    "kind": "open-gallery",
                    "label": "Gallery"
                }],
                "revision": "tray-revision-3"
            }
        }
    }"#
}
