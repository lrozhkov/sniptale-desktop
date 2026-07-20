//! Windows resident pipe listener adapter.
#![cfg(windows)]

use super::AgentEventHub;
use super::pipe_broker::{
    AgentPipeAccept, AgentPipeClock, AgentPipeDuplexListener, AgentPipeEntropy, AgentPipeRejection,
    AgentPipeServerControl, AgentPipeServerError, AgentPipeServerLogSink, AgentPipeServerReport,
    AgentPipeServerRuntime, AgentPipeSessionConfigError, AgentPipeSessionConfigGenerator,
    AgentPipeSessionEffectsFactory, run_agent_pipe_server_with_effects_events_and_log,
};
use super::pipe_peer::{
    ResidentConnectionConsent, accepted_executable_identity, browser_parent_fingerprint,
};
use crate::{
    app::bridge::session::{AgentPipeConnectionContext, AgentPipeSplit},
    ipc::{
        PipeAuthError, PipeSecret, PipeServerAuth, WindowsPipe, WindowsPipeReader,
        WindowsPipeShutdown, WindowsPipeWriter,
    },
};
use std::{
    error::Error,
    fmt,
    io::{Read, Write},
    path::{Path, PathBuf},
    thread,
    time::{Duration, SystemTime},
};
const PIPE_CHALLENGE_TTL_MS: u64 = 5_000;
const PIPE_MAX_SESSIONS: usize = 4;
const PIPE_SESSION_QUEUE_CAPACITY: usize = 16;
const PIPE_ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(25);
const PEER_IDENTITY_UNAVAILABLE: AgentPipeRejection = AgentPipeRejection::new(
    "session-rejected",
    "resident_pipe.peer_identity_unavailable",
    "resident pipe rejected native-host session: peer identity unavailable",
);
const PEER_IDENTITY_REJECTED: AgentPipeRejection = AgentPipeRejection::new(
    "session-rejected",
    "resident_pipe.peer_identity_rejected",
    "resident pipe rejected native-host session: peer identity rejected",
);
const MODE_ADMISSION_DISABLED: AgentPipeRejection = AgentPipeRejection::new(
    "session-rejected",
    "resident_pipe.mode_admission_disabled",
    "resident pipe rejected native-host session: extension admission disabled",
);
#[derive(Debug)]
pub(crate) struct WindowsAgentPipeListener {
    install_id: String,
    current_user_sid: String,
    active_settings: crate::settings::ActiveSettings,
    pending: Option<crate::ipc::WindowsPipe>,
}
#[derive(Debug)]
pub(crate) struct WindowsAgentPipeSessionPipe {
    pipe: WindowsPipe,
    context: AgentPipeConnectionContext,
}
impl WindowsAgentPipeListener {
    pub(crate) fn new(
        install_id: impl Into<String>,
        current_user_sid: impl Into<String>,
        active_settings: crate::settings::ActiveSettings,
    ) -> Self {
        Self {
            install_id: install_id.into(),
            current_user_sid: current_user_sid.into(),
            active_settings,
            pending: None,
        }
    }
}
impl AgentPipeDuplexListener for WindowsAgentPipeListener {
    type Pipe = WindowsAgentPipeSessionPipe;

    fn accept(&mut self) -> Result<AgentPipeAccept<Self::Pipe>, AgentPipeServerError> {
        if !self.active_settings.allows_extension_ingress() {
            return Ok(AgentPipeAccept::Rejected(MODE_ADMISSION_DISABLED));
        }
        if self.pending.is_none() {
            self.pending = Some(
                crate::ipc::create_resident_pipe_listener(
                    &self.install_id,
                    &self.current_user_sid,
                    PIPE_MAX_SESSIONS as u32,
                )
                .map_err(|_| AgentPipeServerError::Accept)?,
            );
        }
        let pipe = self.pending.as_mut().ok_or(AgentPipeServerError::Accept)?;
        if !crate::ipc::poll_resident_pipe_listener(pipe)
            .map_err(|_| AgentPipeServerError::Accept)?
        {
            thread::sleep(PIPE_ACCEPT_POLL_INTERVAL);
            return Ok(AgentPipeAccept::Pending);
        }
        let pipe = self.pending.take().ok_or(AgentPipeServerError::Accept)?;
        let peer = match pipe.connected_client_identity() {
            Ok(peer) => peer,
            Err(_) => return Ok(AgentPipeAccept::Rejected(PEER_IDENTITY_UNAVAILABLE)),
        };
        let context = match verify_connected_pipe_peer(&peer) {
            Ok(context) => context,
            Err(rejection) => return Ok(AgentPipeAccept::Rejected(rejection)),
        };
        crate::ipc::set_resident_pipe_blocking(&pipe).map_err(|_| AgentPipeServerError::Accept)?;
        Ok(AgentPipeAccept::Connected(WindowsAgentPipeSessionPipe {
            pipe,
            context,
        }))
    }
}
fn verify_connected_pipe_peer(
    peer: &crate::ipc::WindowsPipePeerIdentity,
) -> Result<AgentPipeConnectionContext, AgentPipeRejection> {
    let expected = std::env::current_exe().map_err(|_| PEER_IDENTITY_REJECTED)?;
    let observed = PathBuf::from(&peer.image_path);
    let registered = crate::install::trusted_native_host_executable_path();
    let expected_hash = accepted_executable_identity(&observed, &expected, registered.as_deref())
        .ok_or(PEER_IDENTITY_REJECTED)?;
    // `same_windows_path` remains the local fast path. Development may run the resident from the
    // build tree while Chromium starts the installed
    // host. Cross-path admission requires identical bytes or the fixed install path validated by
    // channel/origin plus signature policy (with explicit dev-local trust for unsigned builds).
    let parent = browser_parent_fingerprint(Path::new(&peer.parent_image_path));
    Ok(AgentPipeConnectionContext {
        host_exe_sha256: expected_hash,
        parent_fingerprint: parent.fingerprint,
        parent_label: parent.label,
        parent_category: parent.category,
    })
}
impl Read for WindowsAgentPipeSessionPipe {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.pipe.read(buffer)
    }
}
impl Write for WindowsAgentPipeSessionPipe {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.pipe.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.pipe.flush()
    }
}
impl AgentPipeSplit for WindowsAgentPipeSessionPipe {
    type Reader = WindowsPipeReader;
    type Writer = WindowsPipeWriter;
    type Shutdown = WindowsPipeShutdown;

    fn connection_context(&self) -> Option<&AgentPipeConnectionContext> {
        Some(&self.context)
    }

    fn split_for_session(self) -> std::io::Result<(Self::Reader, Self::Writer, Self::Shutdown)> {
        self.pipe.split_with_shutdown()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WindowsAgentPipeEntropy;

impl AgentPipeEntropy for WindowsAgentPipeEntropy {
    fn next_bytes(&mut self) -> Result<[u8; 16], AgentPipeSessionConfigError> {
        crate::ipc::random_pipe_auth_bytes::<16>().map_err(|_| AgentPipeSessionConfigError::Entropy)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SystemAgentPipeClock;

impl AgentPipeClock for SystemAgentPipeClock {
    fn now_ms(&mut self) -> u64 {
        match SystemTime::now().duration_since(SystemTime::UNIX_EPOCH) {
            Ok(duration) => duration.as_millis().try_into().unwrap_or(u64::MAX),
            Err(_) => 0,
        }
    }
}

#[derive(Debug)]
pub(crate) enum WindowsAgentPipeServerError {
    PipeSecretUnavailable,
    Auth(PipeAuthError),
    Server(AgentPipeServerError),
}

impl fmt::Display for WindowsAgentPipeServerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PipeSecretUnavailable => formatter.write_str("pipe secret is unavailable"),
            Self::Auth(error) => write!(formatter, "pipe auth setup failed: {error}"),
            Self::Server(error) => write!(formatter, "pipe server failed: {error}"),
        }
    }
}

impl Error for WindowsAgentPipeServerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::PipeSecretUnavailable => None,
            Self::Auth(error) => Some(error),
            Self::Server(error) => Some(error),
        }
    }
}

#[derive(Debug)]
pub(crate) struct WindowsAgentPipeServer {
    listener: WindowsAgentPipeListener,
    auth: PipeServerAuth,
    configs: AgentPipeSessionConfigGenerator<WindowsAgentPipeEntropy, SystemAgentPipeClock>,
    consent: ResidentConnectionConsent,
    active_settings: crate::settings::ActiveSettings,
}

impl WindowsAgentPipeServer {
    pub(crate) fn new(
        active_settings: crate::settings::ActiveSettings,
    ) -> Result<Self, WindowsAgentPipeServerError> {
        let identity = crate::install::expected_install_identity();
        let secret = crate::install::load_current_user_pipe_secret(identity.install_id)
            .map_err(|_| WindowsAgentPipeServerError::PipeSecretUnavailable)?;
        let pipe_secret =
            PipeSecret::new(&secret.secret_material).map_err(WindowsAgentPipeServerError::Auth)?;
        let auth = PipeServerAuth::new(
            &secret.current_user_sid,
            &secret.install_id,
            &identity.allowed_origins,
            Some(pipe_secret),
            PIPE_MAX_SESSIONS,
            PIPE_SESSION_QUEUE_CAPACITY,
        )
        .map_err(WindowsAgentPipeServerError::Auth)?;
        Ok(Self {
            listener: WindowsAgentPipeListener::new(
                secret.install_id.clone(),
                secret.current_user_sid.clone(),
                active_settings.clone(),
            ),
            auth,
            configs: AgentPipeSessionConfigGenerator::new(
                WindowsAgentPipeEntropy,
                SystemAgentPipeClock,
                PIPE_CHALLENGE_TTL_MS,
            ),
            consent: ResidentConnectionConsent::default(),
            active_settings,
        })
    }

    pub(crate) fn run_with_effects_events_and_log<E, F>(
        &mut self,
        effects: E,
        events: &AgentEventHub,
        log: Option<&AgentPipeServerLogSink>,
        control: F,
    ) -> Result<AgentPipeServerReport, WindowsAgentPipeServerError>
    where
        E: AgentPipeSessionEffectsFactory,
        F: FnMut(&AgentPipeServerReport) -> AgentPipeServerControl,
    {
        run_agent_pipe_server_with_effects_events_and_log(
            &mut self.listener,
            &mut self.auth,
            &mut self.configs,
            &effects,
            events,
            &AgentPipeServerRuntime {
                log,
                consent: Some(&self.consent),
                active_settings: Some(self.active_settings.clone()),
            },
            control,
        )
        .map_err(WindowsAgentPipeServerError::Server)
    }
}
