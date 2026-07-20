#![deny(unsafe_code)]
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::process::ExitCode;

mod app;
#[cfg(any(windows, test))]
mod assets;
mod audio;
mod capture;
mod diagnostics;
mod hotkeys;
mod install;
mod ipc;
mod protocol;
mod recording;
mod settings;
mod telemetry;
mod transfer;
mod tray;

fn main() -> ExitCode {
    app::run(std::env::args_os())
}
