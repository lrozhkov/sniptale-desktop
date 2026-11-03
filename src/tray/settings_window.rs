//! Settings presentation state and the owner-local system folder picker.

#[path = "settings_window/model.rs"]
pub(crate) mod model;
#[cfg(windows)]
#[path = "settings_window/windows.rs"]
pub(crate) mod windows;
