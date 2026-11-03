//! Windows telemetry adapters and OS event normalization.

#[cfg(windows)]
mod hook_thread;
mod key;
#[cfg(windows)]
mod keyboard_hook;
mod mouse;
#[cfg(windows)]
mod mouse_hook;

#[cfg(any(test, windows))]
pub(crate) use key::WindowsKeyEvent;
#[cfg(test)]
pub(crate) use key::WindowsKeyModifiers;
#[cfg(test)]
pub(crate) use key::WindowsKeyTransition;
#[cfg(any(test, windows))]
pub(crate) use key::sanitize_windows_key_event;
#[cfg(any(test, windows))]
pub(crate) use key::{WindowsKeyObservation, WindowsKeyPhase};
#[cfg(windows)]
pub(crate) use keyboard_hook::WindowsKeyboardTelemetryHook;
#[cfg(any(test, windows))]
pub(crate) use mouse::WindowsMouseButton;
#[cfg(any(test, windows))]
pub(crate) use mouse::{WindowsMouseEvent, WindowsMouseEventKind};
#[cfg(windows)]
pub(crate) use mouse_hook::WindowsMouseTelemetryHook;
#[cfg(windows)]
pub(crate) use mouse_hook::current_boot_ms;
