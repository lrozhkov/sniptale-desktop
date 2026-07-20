//! Tray command-source runtime roles.

#[cfg(any(windows, test))]
pub(crate) mod browser;
#[cfg(any(windows, test))]
pub(crate) mod callback_slot;
pub(crate) mod source;
#[cfg(windows)]
pub(crate) mod windows;

#[cfg(windows)]
pub(crate) use windows::WindowsTrayCommandSource;
