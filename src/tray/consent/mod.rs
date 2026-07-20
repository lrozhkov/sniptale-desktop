//! Tray-owned native connection consent prompt roles.

pub(crate) mod prompt;
#[cfg(windows)]
pub(crate) mod windows;

#[cfg(windows)]
pub(crate) use windows::WindowsConnectionConsentPrompt;
