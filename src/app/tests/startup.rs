use super::super::resident::{
    AgentComponent, AgentStartupInputs, AgentStartupStatus, plan_agent_start,
};
use crate::settings::{ActiveSettings, OperatingMode};

#[test]
fn manual_mode_starts_a_gated_pipe_worker_for_later_reentry() {
    let inputs = AgentStartupInputs {
        install_repair_required: false,
        cache_schema_supported: true,
        active_settings: ActiveSettings::for_test_mode(OperatingMode::Manual),
    };
    let plan = plan_agent_start(&inputs);

    assert_eq!(plan.status, AgentStartupStatus::Operational);
    assert!(plan.components.contains(&AgentComponent::PipeServer));
    assert!(plan.components.contains(&AgentComponent::CaptureWorker));
    assert!(plan.components.contains(&AgentComponent::EncoderWorker));
}

mod initial_browser_handshake_tests {
    use crate::app::bridge::{
        InitialBrowserHandshake, InitialBrowserHandshakeError, read_initial_browser_handshake,
    };
    fn read(
        value: &serde_json::Value,
        origin: &str,
    ) -> Result<InitialBrowserHandshake, InitialBrowserHandshakeError> {
        let bytes = serde_json::to_vec(value).map_err(|_| InitialBrowserHandshakeError::Frame)?;
        let mut frame = u32::try_from(bytes.len())
            .map_err(|_| InitialBrowserHandshakeError::Frame)?
            .to_le_bytes()
            .to_vec();
        frame.extend(bytes);
        read_initial_browser_handshake(&mut std::io::Cursor::new(frame), origin)
    }
    #[test]
    fn admission_binds_the_first_valid_hello_before_any_resident_side_effect()
    -> Result<(), Box<dyn std::error::Error>> {
        let fixtures: serde_json::Value = serde_json::from_str(include_str!(
            "../../../contracts/native/fixtures/positive/contract-fixtures.json"
        ))?;
        let hello = fixtures["ExtensionHelloMessage"].clone();
        let origin = format!(
            "chrome-extension://{}/",
            hello["extensionId"].as_str().ok_or("fixture identity")?
        );
        let admitted = read(&hello, &origin).map_err(|error| format!("{error:?}"))?;
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&admitted.payload)?,
            hello
        );
        assert_eq!(
            admitted.intent,
            crate::protocol::ConnectionIntent::Automatic
        );
        assert!(matches!(
            read(&hello, "chrome-extension://different/"),
            Err(InitialBrowserHandshakeError::Origin)
        ));
        let mut malformed = hello.clone();
        malformed["protocolVersion"] = serde_json::json!(1);
        assert!(matches!(
            read(&malformed, &origin),
            Err(InitialBrowserHandshakeError::Protocol(
                crate::protocol::ParseErrorKind::UnsupportedVersion
            ))
        ));
        let mut missing = hello.clone();
        missing
            .as_object_mut()
            .ok_or("fixture object")?
            .remove("capabilities");
        assert!(matches!(
            read(&missing, &origin),
            Err(InitialBrowserHandshakeError::Protocol(_))
        ));
        assert!(matches!(
            read(&fixtures["ExtensionPingMessage"], &origin),
            Err(InitialBrowserHandshakeError::NotHello)
        ));
        for frame in [
            vec![],
            vec![1, 0],
            vec![255, 255, 255, 255],
            vec![5, 0, 0, 0, b'{'],
        ] {
            assert!(matches!(
                read_initial_browser_handshake(&mut std::io::Cursor::new(frame), &origin),
                Err(InitialBrowserHandshakeError::Frame)
            ));
        }
        assert_eq!(
            InitialBrowserHandshakeError::Frame.phase(),
            "hello-read-rejected"
        );
        assert_eq!(
            InitialBrowserHandshakeError::Origin.phase(),
            "hello-origin-rejected"
        );
        assert_eq!(
            InitialBrowserHandshakeError::NotHello.phase(),
            "hello-contract-rejected"
        );
        Ok(())
    }
}
