use super::super::autostart::{
    AutostartCommandError, evaluate_autostart_command, parse_autostart_command,
};
use super::super::repair::AutostartState;

const EXPECTED_EXE: &str =
    "C:\\Users\\Alice Smith\\AppData\\Local\\Sniptale\\NativeAgent\\sniptale-native-agent.exe";

#[test]
fn autostart_command_parses_quoted_executable_and_args() -> Result<(), AutostartCommandError> {
    let parsed = parse_autostart_command(
        r#""C:\Users\Alice Smith\AppData\Local\Sniptale\NativeAgent\sniptale-native-agent.exe" --native-host chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/"#,
    )?;

    assert_eq!(parsed.executable_path, EXPECTED_EXE);
    assert_eq!(
        parsed.args,
        vec![
            "--native-host".to_owned(),
            "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/".to_owned()
        ]
    );
    Ok(())
}

#[test]
fn autostart_command_accepts_resident_agent_default_mode_only() {
    assert_eq!(
        evaluate_autostart_command(Some(&format!(r#""{EXPECTED_EXE}""#)), EXPECTED_EXE),
        AutostartState::EnabledResidentAgent
    );
    assert_eq!(
        evaluate_autostart_command(Some(EXPECTED_EXE), EXPECTED_EXE),
        AutostartState::PathDrift
    );
}

#[test]
fn autostart_command_classifies_disabled_native_host_path_drift_and_invalid() {
    assert_eq!(
        evaluate_autostart_command(None, EXPECTED_EXE),
        AutostartState::Disabled
    );
    assert_eq!(
        evaluate_autostart_command(Some("  "), EXPECTED_EXE),
        AutostartState::Disabled
    );
    assert_eq!(
        evaluate_autostart_command(
            Some(&format!(r#""{EXPECTED_EXE}" --native-host"#)),
            EXPECTED_EXE
        ),
        AutostartState::NativeHostMode
    );
    assert_eq!(
        evaluate_autostart_command(Some(r#""C:\Temp\sniptale-native-agent.exe""#), EXPECTED_EXE),
        AutostartState::PathDrift
    );
    assert_eq!(
        evaluate_autostart_command(
            Some(&format!(r#""{EXPECTED_EXE}" --repair-registration"#)),
            EXPECTED_EXE
        ),
        AutostartState::InvalidCommand
    );
    assert_eq!(
        evaluate_autostart_command(Some(r#""C:\broken"#), EXPECTED_EXE),
        AutostartState::InvalidCommand
    );
}
