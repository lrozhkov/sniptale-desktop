//! Shared-web presentation and its bounded, typed application seam.

pub(crate) mod model;
pub(crate) mod panel_frame;
#[cfg(any(test, windows))]
pub(crate) mod prompts;
#[cfg(windows)]
mod runtime;
pub(crate) mod settings;
#[cfg(windows)]
pub(crate) use runtime::DesktopHandle;
#[cfg(all(windows, not(test)))]
pub(crate) use runtime::DesktopRuntime;
#[cfg(windows)]
pub(crate) use runtime::show_prompt;
