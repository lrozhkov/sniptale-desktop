//! Native-host handshake compatibility policy.

use crate::protocol::{
    ExtensionHelloMessage, MIN_SUPPORTED_EXTENSION_VERSION, OperationalErrorKind, PROTOCOL_VERSION,
    SETTINGS_SCHEMA_VERSION,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct HandshakeFailure {
    pub(super) kind: OperationalErrorKind,
    pub(super) message: &'static str,
}

pub(super) fn hello_responses(
    session: &mut super::NativeHostSession,
    hello: &ExtensionHelloMessage,
) -> Vec<serde_json::Value> {
    session.handshake_accepted = false;
    session.recording_formats = super::session::NegotiatedRecordingFormats::default();
    session.extension_id = None;
    if !session.accepts_extension_id(&hello.extension_id) {
        return vec![super::handshake_failure(
            HandshakeFailure {
                kind: crate::protocol::OperationalErrorKind::MalformedMessage,
                message: "extension identity does not match authenticated origin",
            },
            None,
        )];
    }
    if let Some(failure) = hello_compatibility_failure(hello) {
        return vec![super::handshake_failure(failure, None)];
    }
    let formats = requested_recording_formats(hello);
    let response = super::hello::app_hello_for_recording_formats(&formats);
    session.handshake_accepted = response["type"] == "app.hello";
    if session.handshake_accepted {
        session.recording_formats = formats;
    }
    session.extension_id = session
        .handshake_accepted
        .then(|| hello.extension_id.clone());
    vec![response]
}

fn requested_recording_formats(
    hello: &ExtensionHelloMessage,
) -> super::session::NegotiatedRecordingFormats {
    let requested = |token: &str| {
        cfg!(any(windows, test)) && hello.capabilities.iter().any(|value| value == token)
    };
    super::session::NegotiatedRecordingFormats {
        cursor_trajectory: requested(crate::protocol::trajectory::CURSOR_TRAJECTORY_CAPABILITY),
        recording_events: requested(crate::protocol::recording_events::RECORDING_EVENTS_CAPABILITY),
    }
}

pub(super) fn hello_compatibility_failure(
    hello: &ExtensionHelloMessage,
) -> Option<HandshakeFailure> {
    if !["desktop-settings-v1", "durable-media-ack-v1"]
        .iter()
        .all(|required| hello.capabilities.iter().any(|value| value == required))
    {
        return Some(HandshakeFailure {
            kind: OperationalErrorKind::HandshakeIncompatibleProtocol,
            message: "required Desktop capabilities are missing",
        });
    }
    if !hello
        .supported_protocol_versions
        .contains(&PROTOCOL_VERSION)
    {
        return Some(HandshakeFailure {
            kind: OperationalErrorKind::HandshakeIncompatibleProtocol,
            message: "no mutually supported protocol version",
        });
    }
    if !hello
        .supported_settings_schema_versions
        .contains(&SETTINGS_SCHEMA_VERSION)
    {
        return Some(HandshakeFailure {
            kind: OperationalErrorKind::HandshakeIncompatibleSettings,
            message: "no mutually supported settings schema version",
        });
    }
    if version_less_than(env!("CARGO_PKG_VERSION"), &hello.min_app_version) {
        return Some(HandshakeFailure {
            kind: OperationalErrorKind::AppUpgradeRequired,
            message: "native app version is below extension minimum",
        });
    }
    if version_less_than(&hello.extension_version, MIN_SUPPORTED_EXTENSION_VERSION) {
        return Some(HandshakeFailure {
            kind: OperationalErrorKind::ExtensionUpgradeRequired,
            message: "extension version is below native app minimum",
        });
    }
    None
}

fn version_less_than(version: &str, minimum: &str) -> bool {
    let Some(version) = version_triplet(version) else {
        return true;
    };
    let Some(minimum) = version_triplet(minimum) else {
        return true;
    };
    version < minimum
}

fn version_triplet(version: &str) -> Option<[u64; 3]> {
    let mut triplet = [0_u64; 3];
    let mut count = 0;
    for (index, part) in version.split('.').enumerate() {
        if index >= triplet.len() || part.is_empty() {
            return None;
        }
        triplet[index] = part.parse().ok()?;
        count += 1;
    }
    if count == 0 { None } else { Some(triplet) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hello() -> ExtensionHelloMessage {
        ExtensionHelloMessage {
            connection_intent: crate::protocol::ConnectionIntent::Automatic,
            protocol_version: PROTOCOL_VERSION,
            settings_schema_version: SETTINGS_SCHEMA_VERSION,
            extension_version: MIN_SUPPORTED_EXTENSION_VERSION.to_owned(),
            extension_id: "abcdefghijklmnopqrstuvwxzyabcdef".to_owned(),
            min_app_version: env!("CARGO_PKG_VERSION").to_owned(),
            supported_protocol_versions: vec![PROTOCOL_VERSION],
            supported_settings_schema_versions: vec![SETTINGS_SCHEMA_VERSION],
            capabilities: vec![
                "desktop-settings-v1".to_owned(),
                "durable-media-ack-v1".to_owned(),
            ],
        }
    }

    #[test]
    fn compatible_hello_has_no_failure() {
        assert_eq!(hello_compatibility_failure(&hello()), None);
    }

    #[test]
    fn hello_policy_reports_each_version_failure() {
        let mut incompatible_protocol = hello();
        incompatible_protocol.supported_protocol_versions = vec![PROTOCOL_VERSION + 1];
        assert_eq!(
            hello_compatibility_failure(&incompatible_protocol).map(|failure| failure.kind),
            Some(OperationalErrorKind::HandshakeIncompatibleProtocol)
        );

        let mut incompatible_settings = hello();
        incompatible_settings.supported_settings_schema_versions =
            vec![SETTINGS_SCHEMA_VERSION + 1];
        assert_eq!(
            hello_compatibility_failure(&incompatible_settings).map(|failure| failure.kind),
            Some(OperationalErrorKind::HandshakeIncompatibleSettings)
        );

        let mut app_upgrade = hello();
        app_upgrade.min_app_version = "999.0.0".to_owned();
        assert_eq!(
            hello_compatibility_failure(&app_upgrade).map(|failure| failure.kind),
            Some(OperationalErrorKind::AppUpgradeRequired)
        );

        let mut extension_upgrade = hello();
        extension_upgrade.extension_version = "0.0.1".to_owned();
        assert_eq!(
            hello_compatibility_failure(&extension_upgrade).map(|failure| failure.kind),
            Some(OperationalErrorKind::ExtensionUpgradeRequired)
        );
    }

    #[test]
    fn history_formats_are_independent_and_require_the_exact_requested_token()
    -> Result<(), Box<dyn std::error::Error>> {
        for (cursor, events) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut request = hello();
            request.capabilities.extend([
                "cursor-trajectory-v2".to_owned(),
                "recording-events-v2".to_owned(),
            ]);
            if cursor {
                request.capabilities.push("cursor-trajectory-v1".to_owned());
            }
            if events {
                request.capabilities.push("recording-events-v1".to_owned());
            }
            let mut session = super::super::NativeHostSession::default();
            let responses = hello_responses(&mut session, &request);
            assert_eq!(responses.len(), 1);
            assert_eq!(responses[0]["type"], "app.hello");
            assert!(session.handshake_accepted);
            assert_eq!(session.recording_formats.cursor_trajectory, cursor);
            assert_eq!(session.recording_formats.recording_events, events);
            let capabilities = &responses[0]["capabilities"];
            assert_eq!(capabilities.get("attachments").is_some(), cursor);
            assert_eq!(capabilities.get("eventAttachments").is_some(), events);
            for (field, formats_key, token, enabled) in [
                (
                    "attachments",
                    "cursorTrajectoryFormats",
                    "cursor-trajectory-v1",
                    cursor,
                ),
                (
                    "eventAttachments",
                    "recordingEventFormats",
                    "recording-events-v1",
                    events,
                ),
            ] {
                if enabled {
                    assert_eq!(
                        capabilities[field],
                        serde_json::json!({
                            formats_key: [token], "maxBlockBytes": 65536,
                            "maxBlockRecords": 512, "maxChunkRawBytes": 32768,
                            "maxManifestEntries": 32,
                        })
                    );
                }
            }
            assert!(matches!(
                crate::protocol::parse_wire_message(&serde_json::to_vec(&responses[0])?)?,
                crate::protocol::WireMessage::AppHello(_)
            ));
        }
        Ok(())
    }

    #[test]
    fn repeated_hello_replaces_previous_format_negotiation() {
        let mut request = hello();
        request.capabilities.extend([
            "cursor-trajectory-v1".to_owned(),
            "recording-events-v1".to_owned(),
        ]);
        let mut session = super::super::NativeHostSession::default();
        hello_responses(&mut session, &request);
        assert!(session.recording_formats.cursor_trajectory);
        assert!(session.recording_formats.recording_events);
        request
            .capabilities
            .retain(|value| value != "cursor-trajectory-v1");
        let response = hello_responses(&mut session, &request);
        assert!(!session.recording_formats.cursor_trajectory);
        assert!(session.recording_formats.recording_events);
        assert!(response[0]["capabilities"].get("attachments").is_none());
        let response = hello_responses(&mut session, &hello());
        assert!(!session.recording_formats.recording_events);
        assert!(
            response[0]["capabilities"]
                .get("eventAttachments")
                .is_none()
        );
    }

    #[test]
    fn rejected_hello_cannot_retain_previously_negotiated_history_access() {
        for wrong_origin in [false, true] {
            let mut request = hello();
            request.capabilities.extend([
                "cursor-trajectory-v1".to_owned(),
                "recording-events-v1".to_owned(),
            ]);
            let mut session = super::super::NativeHostSession::default();
            hello_responses(&mut session, &request);
            assert!(session.handshake_accepted);
            if wrong_origin {
                session.bind_authenticated_origin(
                    "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/",
                );
            } else {
                request.supported_protocol_versions.clear();
            }
            let responses = hello_responses(&mut session, &request);
            assert_eq!(responses[0]["type"], "app.operation.failed");
            assert!(!session.handshake_accepted);
            assert!(session.extension_id.is_none());
            assert!(!session.recording_formats.cursor_trajectory);
            assert!(!session.recording_formats.recording_events);
        }
    }

    #[test]
    fn unsolicited_hello_has_no_history_advertisement() {
        let response = super::super::app_hello();
        assert!(response["capabilities"].get("attachments").is_none());
        assert!(response["capabilities"].get("eventAttachments").is_none());
    }
}
