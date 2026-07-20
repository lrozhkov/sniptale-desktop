//! Channel-scoped runtime install identity.

use super::registry::{
    InstallChannel, expected_allowed_extension_origins, expected_host_name,
    expected_install_channel, expected_install_id,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InstallRuntimeIdentity {
    pub(crate) channel: InstallChannel,
    pub(crate) install_id: &'static str,
    pub(crate) host_name: &'static str,
    pub(crate) allowed_origins: [&'static str; 1],
}

impl InstallRuntimeIdentity {
    pub(crate) const fn for_channel_and_origins(
        channel: InstallChannel,
        allowed_origins: [&'static str; 1],
    ) -> Self {
        Self {
            channel,
            install_id: expected_install_id(channel),
            host_name: expected_host_name(channel),
            allowed_origins,
        }
    }

    pub(crate) fn allows_extension_origin(&self, origin: &str) -> bool {
        self.allowed_origins.contains(&origin)
    }
}

pub(crate) fn expected_install_identity() -> InstallRuntimeIdentity {
    InstallRuntimeIdentity::for_channel_and_origins(
        expected_install_channel(),
        expected_allowed_extension_origins(),
    )
}
