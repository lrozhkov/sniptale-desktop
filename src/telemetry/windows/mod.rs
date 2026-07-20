//! Windows telemetry adapters and OS event normalization.

mod key;
#[cfg(windows)]
mod keyboard_hook;
mod mouse;
#[cfg(windows)]
mod mouse_hook;

#[cfg(any(test, windows))]
pub(crate) use key::sanitize_windows_key_event;
#[cfg(test)]
pub(crate) use key::{WindowsKeyEvent, WindowsKeyModifiers};
#[cfg(windows)]
pub(crate) use keyboard_hook::WindowsKeyboardTelemetryHook;
#[cfg(test)]
pub(crate) use mouse::WindowsMouseButton;
#[cfg(any(test, windows))]
pub(crate) use mouse::{WindowsMouseEvent, WindowsMouseEventKind};
#[cfg(windows)]
pub(crate) use mouse_hook::WindowsMouseTelemetryHook;
