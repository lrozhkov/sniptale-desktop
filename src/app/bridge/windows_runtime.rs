//! Windows native-host pipe forwarding runtime.
#![cfg(all(windows, not(test)))]

use std::{
    io::{Read, Write},
    process::{Command, Stdio},
    thread,
    time::Duration,
};

use crate::{
    install::{
        expected_install_identity, load_current_user_pipe_secret,
        verify_current_runtime_manifest_identity,
    },
    ipc::{PipeSecret, ResidentPipeConnectError},
};

use super::{
    NativeHostRunError,
    runtime::{
        NativeHostPipeAuthConfig, NativeHostRuntimeLog, bridge_client_nonce,
        run_authenticated_pipe_forwarding_bridge_with_log,
    },
    startup::connect_pipe_with_typed_agent_startup,
};

const PIPE_CONNECT_ATTEMPTS: u8 = 100;
const PIPE_CONNECT_RETRY_DELAY: Duration = Duration::from_millis(100);

#[expect(
    clippy::too_many_lines,
    reason = "Keep install validation, initial hello, explicit-Quit guard, and pipe authentication in one auditable startup order."
)]
pub(super) fn run_pipe_forwarding_bridge(
    reader: &mut (impl Read + Send),
    writer: &mut (impl Write + Send),
    extension_origin: &str,
    log: Option<&dyn NativeHostRuntimeLog>,
) -> Result<(), NativeHostRunError> {
    log_phase(
        log,
        "install-identity-verify",
        "verifying native host install identity",
    );
    verify_current_runtime_manifest_identity().map_err(NativeHostRunError::InstallIdentityDrift)?;
    log_phase(log, "hello-read", "reading initial browser handshake");
    let identity = expected_install_identity();
    let initial = crate::ipc::with_blocking_read_deadline(Duration::from_secs(15), || {
        super::startup::read_initial_browser_handshake(reader, extension_origin)
    })
    .map_err(|_| super::startup::InitialBrowserHandshakeError::Frame)
    .and_then(|result| result)
    .map_err(|error| {
        log_phase(
            log,
            error.phase(),
            &format!("initial browser handshake rejected: {error:?}"),
        );
        NativeHostRunError::PipeAuthenticationUnavailable
    })?;
    log_phase(
        log,
        "browser-startup-preference",
        "checking browser startup preference",
    );
    let startup_path = crate::settings::browser_startup_preference_path()
        .ok_or(NativeHostRunError::PipeAuthenticationUnavailable)?;
    let start_allowed = crate::settings::browser_start_allowed(&startup_path)
        .map_err(|_| NativeHostRunError::PipeAuthenticationUnavailable)?;
    if !start_allowed && initial.intent == crate::protocol::ConnectionIntent::Automatic {
        for value in [
            crate::app::responses::app_hello(),
            crate::app::responses::connection_closed(),
        ] {
            let payload = serde_json::to_vec(&value)
                .map_err(|_| NativeHostRunError::PipeAuthenticationUnavailable)?;
            crate::ipc::write_payload(
                writer,
                &payload,
                crate::protocol::HOST_TO_EXTENSION_MAX_MESSAGE_BYTES,
            )
            .map_err(|_| NativeHostRunError::PipeAuthenticationUnavailable)?;
        }
        return Ok(());
    }
    let mut frame = u32::try_from(initial.payload.len())
        .map_err(|_| NativeHostRunError::PipeAuthenticationUnavailable)?
        .to_le_bytes()
        .to_vec();
    frame.extend(initial.payload);
    let mut replay = std::io::Cursor::new(frame).chain(reader);
    log_phase(
        log,
        "pipe-secret-load",
        "loading resident pipe authentication material",
    );
    let secret = load_current_user_pipe_secret(identity.install_id)
        .map_err(|_| NativeHostRunError::PipeAuthenticationUnavailable)?;
    let pipe_secret = PipeSecret::new(&secret.secret_material)
        .map_err(|_| NativeHostRunError::PipeAuthenticationUnavailable)?;
    log_phase(log, "pipe-connect", "connecting to resident agent pipe");
    let connected = connect_pipe_with_typed_agent_startup(
        PIPE_CONNECT_ATTEMPTS,
        || crate::ipc::connect_resident_pipe(&secret.install_id),
        || spawn_resident_agent_once().map_err(|_| ()),
        || thread::sleep(PIPE_CONNECT_RETRY_DELAY),
        ResidentPipeConnectError::should_start_resident,
    )
    .map_err(|error| {
        log_pipe_connect_failure(log, error);
        NativeHostRunError::PipeAuthenticationUnavailable
    })?;
    let agent_pipe = connected.pipe;
    log_phase(
        log,
        "pipe-connected",
        if connected.started_agent {
            "connected to resident agent pipe after starting agent"
        } else {
            "connected to resident agent pipe"
        },
    );
    let config = NativeHostPipeAuthConfig {
        current_user_sid: secret.current_user_sid,
        install_id: secret.install_id,
        extension_origin: extension_origin.to_owned(),
        client_nonce: bridge_client_nonce()
            .map_err(|_| NativeHostRunError::PipeAuthenticationUnavailable)?,
        secret: pipe_secret,
    };
    log_phase(log, "pipe-auth", "authenticating resident agent pipe");
    run_authenticated_pipe_forwarding_bridge_with_log(
        &mut replay,
        writer,
        agent_pipe,
        &config,
        log,
    )?;
    log_phase(log, "forwarded", "native messaging forwarding completed");
    Ok(())
}

fn log_phase(log: Option<&dyn NativeHostRuntimeLog>, phase: &'static str, message: &str) {
    if let Some(log) = log {
        log.phase(phase, message);
    }
}

fn log_pipe_connect_failure(
    log: Option<&dyn NativeHostRuntimeLog>,
    error: super::startup::BridgeStartupError,
) {
    let phase = match error {
        super::startup::BridgeStartupError::PipeBusy => "pipe-busy-timeout",
        super::startup::BridgeStartupError::PipeUnavailable => "pipe-connect-timeout",
        super::startup::BridgeStartupError::InvalidAttemptLimit => "pipe-connect-invalid",
        #[cfg(test)]
        super::startup::BridgeStartupError::OriginRejected => "origin-rejected",
    };
    log_phase(log, phase, "resident agent pipe connection failed");
}

fn spawn_resident_agent_once() -> std::io::Result<()> {
    let executable = std::env::current_exe()?;
    Command::new(executable)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}
