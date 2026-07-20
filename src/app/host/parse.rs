//! Process mode argument parsing.

use std::{ffi::OsString, path::PathBuf};

use crate::install::allowed_extension_origin;

use super::{ModeError, ProcessMode};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct SupportModeOptions {
    pub(super) log_level: Option<String>,
    pub(super) log_dir: Option<PathBuf>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ParsedMode {
    pub(super) mode: ProcessMode,
    pub(super) support_options: SupportModeOptions,
    pub(super) native_host_origin: Option<String>,
}

pub(super) fn parse_mode(
    args: impl IntoIterator<Item = OsString>,
) -> Result<ParsedMode, ModeError> {
    let mut args = args.into_iter();
    let _program = args.next();
    let Some(mode_arg) = args.next() else {
        return Ok(ParsedMode {
            mode: ProcessMode::Agent,
            support_options: SupportModeOptions::default(),
            native_host_origin: None,
        });
    };
    let Some(mode_value) = mode_arg.to_str() else {
        return Err(ModeError::UnsupportedArgument);
    };
    let mode = if mode_value == "--native-host" {
        let native_host_origin = validate_native_host_browser_args(args)?;
        ParsedMode {
            mode: ProcessMode::NativeHost,
            support_options: SupportModeOptions::default(),
            native_host_origin: Some(native_host_origin),
        }
    } else if allowed_extension_origin(mode_value) {
        let native_host_origin =
            validate_native_host_browser_args(std::iter::once(mode_arg).chain(args))?;
        ParsedMode {
            mode: ProcessMode::NativeHost,
            support_options: SupportModeOptions::default(),
            native_host_origin: Some(native_host_origin),
        }
    } else {
        match mode_value {
            "--repair-registration" => parse_terminal_mode(args, ProcessMode::RepairRegistration)?,
            "--version-json" => parse_terminal_mode(args, ProcessMode::VersionJson)?,
            "--doctor-json" => parse_terminal_mode(args, ProcessMode::DoctorJson)?,
            "--collect-diagnostics" => parse_terminal_mode(args, ProcessMode::CollectDiagnostics)?,
            _ => return Err(ModeError::UnsupportedArgument),
        }
    };
    Ok(mode)
}

fn parse_terminal_mode(
    mut args: impl Iterator<Item = OsString>,
    mode: ProcessMode,
) -> Result<ParsedMode, ModeError> {
    let mut support_options = SupportModeOptions::default();
    while let Some(arg) = args.next() {
        let Some(value) = arg.to_str() else {
            return Err(ModeError::UnsupportedArgument);
        };
        if value == "--log-level" {
            let level = args.next().ok_or(ModeError::UnsupportedArgument)?;
            support_options.log_level = Some(
                level
                    .to_str()
                    .ok_or(ModeError::UnsupportedArgument)?
                    .to_owned(),
            );
        } else if let Some(level) = value.strip_prefix("--log-level=") {
            if level.is_empty() {
                return Err(ModeError::UnsupportedArgument);
            }
            support_options.log_level = Some(level.to_owned());
        } else if value == "--log-dir" {
            let directory = args.next().ok_or(ModeError::UnsupportedArgument)?;
            support_options.log_dir = Some(non_empty_path(directory)?);
        } else if let Some(directory) = value.strip_prefix("--log-dir=") {
            if directory.is_empty() {
                return Err(ModeError::UnsupportedArgument);
            }
            support_options.log_dir = Some(PathBuf::from(directory));
        } else {
            return Err(ModeError::UnsupportedArgument);
        }
    }
    Ok(ParsedMode {
        mode,
        support_options,
        native_host_origin: None,
    })
}

fn non_empty_path(value: OsString) -> Result<PathBuf, ModeError> {
    if value.is_empty() {
        Err(ModeError::UnsupportedArgument)
    } else {
        Ok(PathBuf::from(value))
    }
}

fn validate_native_host_browser_args(
    args: impl Iterator<Item = OsString>,
) -> Result<String, ModeError> {
    let mut origin = None;
    for arg in args {
        let Some(value) = arg.to_str() else {
            return Err(ModeError::UnsupportedArgument);
        };
        if let Some(parent_window) = value.strip_prefix("--parent-window=") {
            validate_parent_window_arg(parent_window)?;
            continue;
        }
        if origin.is_some() || !allowed_extension_origin(value) {
            return Err(ModeError::UnsupportedArgument);
        }
        origin = Some(value.to_owned());
    }
    origin.ok_or(ModeError::UnsupportedArgument)
}

fn validate_parent_window_arg(value: &str) -> Result<(), ModeError> {
    if !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()) {
        Ok(())
    } else {
        Err(ModeError::UnsupportedArgument)
    }
}
