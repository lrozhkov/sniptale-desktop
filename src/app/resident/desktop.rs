//! Desktop presentation coordination over existing capture and publication owners.

pub(crate) mod dispatch;
pub(super) mod projection;

pub(super) mod backend;
#[cfg(windows)]
pub(super) mod session;
