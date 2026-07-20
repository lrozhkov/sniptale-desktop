//! Resident startup policy derived from install and cache health.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AgentComponent {
    Tray,
    AssetPublisher,
    Hotkeys,
    PipeServer,
    CaptureWorker,
    EncoderWorker,
    AudioWorker,
    TransferWorker,
    TelemetryWorker,
}

#[derive(Clone, Debug)]
pub(crate) struct AgentStartupInputs {
    pub(crate) install_repair_required: bool,
    pub(crate) cache_schema_supported: bool,
    pub(crate) active_settings: crate::settings::ActiveSettings,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AgentStartupPlan {
    pub(crate) components: Vec<AgentComponent>,
    pub(crate) status: AgentStartupStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AgentStartupStatus {
    Operational,
    RepairRequired,
    UnsupportedCacheSchema,
    RepairRequiredAndUnsupportedCacheSchema,
}

pub(crate) fn plan_agent_start(inputs: &AgentStartupInputs) -> AgentStartupPlan {
    let status = startup_status(inputs);
    let extension_admission_allowed = inputs.active_settings.allows_extension_ingress();
    let components = if status == AgentStartupStatus::Operational && extension_admission_allowed {
        vec![
            AgentComponent::Tray,
            AgentComponent::AssetPublisher,
            AgentComponent::Hotkeys,
            AgentComponent::CaptureWorker,
            AgentComponent::TelemetryWorker,
            AgentComponent::EncoderWorker,
            AgentComponent::AudioWorker,
            AgentComponent::TransferWorker,
            AgentComponent::PipeServer,
        ]
    } else if status == AgentStartupStatus::Operational {
        vec![
            AgentComponent::Tray,
            AgentComponent::PipeServer,
            AgentComponent::AssetPublisher,
            AgentComponent::Hotkeys,
            AgentComponent::CaptureWorker,
            AgentComponent::TelemetryWorker,
            AgentComponent::EncoderWorker,
            AgentComponent::AudioWorker,
            AgentComponent::TransferWorker,
        ]
    } else {
        vec![AgentComponent::Tray, AgentComponent::PipeServer]
    };
    AgentStartupPlan { components, status }
}

pub(crate) fn runtime_agent_startup_plan() -> AgentStartupPlan {
    plan_agent_start(&runtime_agent_startup_inputs())
}

pub(crate) fn runtime_agent_startup_inputs() -> AgentStartupInputs {
    let cache =
        crate::settings::native_cache_health_from_path(crate::app::platform::native_cache_path());
    let profile =
        crate::settings::ensure_local_profile_from_path(crate::app::platform::local_profile_path());
    let install_repair_required =
        crate::install::current_repair_registration_report().repair_required;
    let (profile_repair_required, active_settings) = match profile {
        Ok(profile) => (
            false,
            crate::settings::ActiveSettings::from_profile(&profile),
        ),
        Err(_) => (
            true,
            crate::settings::ActiveSettings::extension_managed_default(),
        ),
    };
    AgentStartupInputs {
        // A corrupt or newer local profile is a settings-repair condition, not a cache-schema
        // condition. Keeping those states distinct prevents a misleading recovery path.
        install_repair_required: install_repair_required || profile_repair_required,
        cache_schema_supported: cache.operations_supported,
        active_settings,
    }
}

const fn startup_status(inputs: &AgentStartupInputs) -> AgentStartupStatus {
    match (
        inputs.install_repair_required,
        inputs.cache_schema_supported,
    ) {
        (false, true) => AgentStartupStatus::Operational,
        (true, true) => AgentStartupStatus::RepairRequired,
        (false, false) => AgentStartupStatus::UnsupportedCacheSchema,
        (true, false) => AgentStartupStatus::RepairRequiredAndUnsupportedCacheSchema,
    }
}
