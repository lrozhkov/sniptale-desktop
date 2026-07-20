//! Coordinated stop, cancel, and cleanup owner.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ShutdownStep {
    StopAcceptingPipeClients,
    DisableHotkeys,
    HideTray,
    CancelCapture,
    StopRecording,
    FlushTransfer,
    StopTelemetry,
    PersistTechnicalState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActiveWork {
    Capture,
    Recording,
    Transfer,
    Telemetry,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ShutdownInputs {
    pub(crate) active_work: Vec<ActiveWork>,
}

pub(crate) fn shutdown_sequence(inputs: &ShutdownInputs) -> Vec<ShutdownStep> {
    let mut steps = vec![
        ShutdownStep::StopAcceptingPipeClients,
        ShutdownStep::DisableHotkeys,
        ShutdownStep::HideTray,
    ];
    if inputs.active_work.contains(&ActiveWork::Capture) {
        steps.push(ShutdownStep::CancelCapture);
    }
    if inputs.active_work.contains(&ActiveWork::Recording) {
        steps.push(ShutdownStep::StopRecording);
    }
    if inputs.active_work.contains(&ActiveWork::Transfer) {
        steps.push(ShutdownStep::FlushTransfer);
    }
    if inputs.active_work.contains(&ActiveWork::Telemetry) {
        steps.push(ShutdownStep::StopTelemetry);
    }
    steps.push(ShutdownStep::PersistTechnicalState);
    steps
}
