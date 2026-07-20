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

pub(super) fn hello_compatibility_failure(
    hello: &ExtensionHelloMessage,
) -> Option<HandshakeFailure> {
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
            protocol_version: PROTOCOL_VERSION,
            settings_schema_version: SETTINGS_SCHEMA_VERSION,
            extension_version: MIN_SUPPORTED_EXTENSION_VERSION.to_owned(),
            extension_id: "abcdefghijklmnopqrstuvwxzyabcdef".to_owned(),
            min_app_version: env!("CARGO_PKG_VERSION").to_owned(),
            supported_protocol_versions: vec![PROTOCOL_VERSION],
            supported_settings_schema_versions: vec![SETTINGS_SCHEMA_VERSION],
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
}
