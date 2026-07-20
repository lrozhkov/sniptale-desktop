//! Global hotkey registration and dispatch.

mod dispatch;
mod registry;
#[cfg(any(windows, test))]
mod runtime;
mod settings;
#[cfg(test)]
mod tests;
#[cfg(windows)]
mod windows_hotkey;

pub(crate) use dispatch::{HotkeyBinding, HotkeyError};
#[cfg(any(windows, test))]
pub(crate) use registry::HotkeyPlanRoute;
#[cfg(any(windows, test))]
pub(crate) use registry::RegisteredHotkey;
pub(crate) use registry::{HotkeyRegistrationPlan, HotkeyRegistry};
#[cfg(any(windows, test))]
pub(crate) use runtime::{HotkeyEventSource, HotkeyRegistrar, HotkeyRuntime, HotkeyRuntimeError};
pub(crate) use settings::bindings_from_action_catalog;
#[cfg(any(windows, test))]
pub(crate) use settings::bindings_from_manual_hotkeys;
#[cfg(windows)]
pub(crate) use windows_hotkey::WindowsHotkeyRegistrar;
