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
