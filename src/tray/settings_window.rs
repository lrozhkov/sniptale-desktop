//! Native settings-window presentation state; persistence stays in `settings`.

#[cfg(any(windows, test))]
#[path = "settings_window/control_chrome.rs"]
pub(crate) mod control_chrome;
#[cfg(any(windows, test))]
#[path = "settings_window/hotkey_capture.rs"]
pub(crate) mod hotkey_capture;
#[cfg(any(windows, test))]
#[path = "settings_window/layout.rs"]
pub(crate) mod layout;
#[path = "settings_window/model.rs"]
pub(crate) mod model;
#[cfg(any(windows, test))]
#[path = "settings_window/palette.rs"]
pub(crate) mod palette;
#[cfg(any(windows, test))]
#[path = "settings_window/render_recovery.rs"]
pub(crate) mod render_recovery;
#[cfg(windows)]
#[path = "settings_window/windows.rs"]
pub(crate) mod windows;
