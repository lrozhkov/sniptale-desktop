//! Tray menu model, action registry, render plan, and native menu adapter roles.

pub(crate) mod actions;
#[cfg(any(windows, test))]
pub(crate) mod extension;
#[cfg(any(windows, test))]
pub(crate) mod manual;
#[cfg(any(windows, test))]
mod mode;
pub(crate) mod model;
#[cfg(any(windows, test))]
pub(crate) mod render;
#[cfg(windows)]
pub(crate) mod windows;
