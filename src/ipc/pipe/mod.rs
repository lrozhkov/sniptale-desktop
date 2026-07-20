//! Resident pipe authentication, naming, and platform adapters.

pub(crate) mod auth_mac;
pub(crate) mod client;
pub(crate) mod handshake;
#[cfg(any(windows, test))]
pub(crate) mod name;
#[cfg(any(windows, test))]
pub(crate) mod security;
pub(crate) mod server;
pub(crate) mod server_types;
#[cfg(windows)]
pub(crate) mod server_windows;
#[cfg(windows)]
pub(crate) mod windows_entropy;
#[cfg(windows)]
pub(crate) mod windows_pipe;
#[cfg(windows)]
pub(crate) mod windows_pipe_handles;
#[cfg(windows)]
pub(crate) mod windows_pipe_overlapped;
#[cfg(windows)]
pub(crate) mod windows_pipe_peer;
#[cfg(windows)]
pub(crate) mod windows_pipe_security;
