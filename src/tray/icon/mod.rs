//! Tray icon state planning and native `Shell_NotifyIcon` adapter roles.

pub(crate) mod state;
#[cfg(windows)]
pub(crate) mod windows;
