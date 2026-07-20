//! Windows resident pipe accept adapter.
#![cfg(windows)]

use std::io;

use super::{name::PipeEndpointName, windows_pipe::WindowsPipe};

pub(crate) fn create_resident_pipe_listener(
    install_id: &str,
    current_user_sid: &str,
    max_instances: u32,
) -> io::Result<WindowsPipe> {
    let endpoint = PipeEndpointName::for_install(install_id)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid install id"))?;
    WindowsPipe::create_server_nonblocking(&endpoint, current_user_sid, max_instances)
}

#[cfg(not(test))]
pub(crate) fn create_resident_agent_singleton_guard(
    install_id: &str,
    current_user_sid: &str,
) -> io::Result<WindowsPipe> {
    let endpoint = PipeEndpointName::for_agent_singleton(install_id)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid install id"))?;
    WindowsPipe::create_server_nonblocking(&endpoint, current_user_sid, 1)
}

pub(crate) fn poll_resident_pipe_listener(pipe: &mut WindowsPipe) -> io::Result<bool> {
    pipe.try_wait_for_client()
}

pub(crate) fn set_resident_pipe_blocking(pipe: &WindowsPipe) -> io::Result<()> {
    pipe.set_blocking_wait_mode()
}
