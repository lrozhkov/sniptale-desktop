//! App, capability, and install-health status owner.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProcessMode {
    Agent,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConnectionHealth {
    Connected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InstallHealth {
    Healthy,
    RepairRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CapabilityHealth {
    Available,
    Degraded,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LastErrorSummary {
    pub(crate) code: String,
    pub(crate) phase: String,
    pub(crate) recoverable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HealthSnapshot {
    pub(crate) app_version: String,
    pub(crate) protocol_version: u32,
    pub(crate) settings_schema_version: u32,
    pub(crate) cache_schema_version: u32,
    pub(crate) process_mode: ProcessMode,
    pub(crate) connection: ConnectionHealth,
    pub(crate) install: InstallHealth,
    pub(crate) capture: CapabilityHealth,
    pub(crate) recording: CapabilityHealth,
    pub(crate) audio: CapabilityHealth,
    pub(crate) last_error: Option<LastErrorSummary>,
}

impl HealthSnapshot {
    pub(crate) fn operational(&self) -> bool {
        self.connection == ConnectionHealth::Connected
            && self.install == InstallHealth::Healthy
            && self.capture != CapabilityHealth::Unavailable
            && self.recording != CapabilityHealth::Unavailable
    }

    pub(crate) fn repair_required(&self) -> bool {
        self.install == InstallHealth::RepairRequired
    }
}
