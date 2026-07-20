use crate::ipc::{PipeEndpointName, PipeNameError};

#[test]
fn pipe_endpoint_name_is_current_machine_pipe_scoped_by_install_id() -> Result<(), PipeNameError> {
    let endpoint = PipeEndpointName::for_install("install_01-A")?;

    assert_eq!(
        endpoint.as_str(),
        r"\\.\pipe\sniptale-native-agent-install_01-A"
    );
    Ok(())
}

#[test]
fn agent_singleton_endpoint_is_install_scoped_and_distinct() -> Result<(), PipeNameError> {
    let endpoint = PipeEndpointName::for_agent_singleton("install_01-A")?;

    assert_eq!(
        endpoint.as_str(),
        r"\\.\pipe\sniptale-native-agent-install_01-A-agent-singleton"
    );
    assert_ne!(endpoint, PipeEndpointName::for_install("install_01-A")?);
    Ok(())
}

#[test]
fn pipe_endpoint_name_rejects_path_injection_and_ambiguous_ids() {
    for install_id in [
        "",
        "short",
        "install\\escape",
        "install/escape",
        "install:escape",
        "инсталляция",
    ] {
        assert_eq!(
            PipeEndpointName::for_install(install_id),
            Err(PipeNameError::InvalidInstallId)
        );
    }
}
