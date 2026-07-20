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
    log_phase(
        log,
        "pipe-secret-load",
        "loading resident pipe authentication material",
    );
    let identity = expected_install_identity();
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
    run_authenticated_pipe_forwarding_bridge_with_log(reader, writer, agent_pipe, &config, log)?;
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
