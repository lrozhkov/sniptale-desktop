//! Autostart command validation owner.

use super::repair::AutostartState;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AutostartCommand {
    pub(crate) executable_path: String,
    pub(crate) args: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AutostartCommandError {
    Empty,
    UnterminatedQuote,
}

pub(crate) fn evaluate_autostart_command(
    command: Option<&str>,
    expected_executable_path: &str,
) -> AutostartState {
    let Some(command) = command else {
        return AutostartState::Disabled;
    };
    if command.trim().is_empty() {
        return AutostartState::Disabled;
    }

    let Ok(parsed) = parse_autostart_command(command) else {
        return AutostartState::InvalidCommand;
    };
    if parsed.executable_path != expected_executable_path {
        return AutostartState::PathDrift;
    }
    if parsed.args.iter().any(|arg| arg == "--native-host") {
        return AutostartState::NativeHostMode;
    }
    if parsed.args.is_empty() {
        AutostartState::EnabledResidentAgent
    } else {
        AutostartState::InvalidCommand
    }
}

pub(crate) fn parse_autostart_command(
    command: &str,
) -> Result<AutostartCommand, AutostartCommandError> {
    let tokens = split_command_line(command)?;
    let mut tokens = tokens.into_iter();
    let Some(executable_path) = tokens.next() else {
        return Err(AutostartCommandError::Empty);
    };
    Ok(AutostartCommand {
        executable_path,
        args: tokens.collect(),
    })
}

fn split_command_line(command: &str) -> Result<Vec<String>, AutostartCommandError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut token_started = false;

    for character in command.chars() {
        match character {
            '"' => {
                in_quotes = !in_quotes;
                token_started = true;
            }
            character if character.is_whitespace() && !in_quotes => {
                if token_started {
                    tokens.push(std::mem::take(&mut current));
                    token_started = false;
                }
            }
            _ => {
                current.push(character);
                token_started = true;
            }
        }
    }

    if in_quotes {
        return Err(AutostartCommandError::UnterminatedQuote);
    }
    if token_started {
        tokens.push(current);
    }
    Ok(tokens)
}
