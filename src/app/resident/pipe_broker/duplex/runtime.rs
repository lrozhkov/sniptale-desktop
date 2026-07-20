//! Resident pipe server runtime configuration.

use super::super::logging::AgentPipeServerLogSink;
use crate::app::{bridge::session::AgentPipeConnectionConsent, responses::NativeHostSideEffects};

#[derive(Clone)]
pub(crate) struct AgentPipeServerRuntime<'a> {
    pub(crate) log: Option<&'a AgentPipeServerLogSink>,
    pub(crate) consent: Option<&'a dyn AgentPipeConnectionConsent>,
    pub(crate) active_settings: Option<crate::settings::ActiveSettings>,
}

pub(crate) trait AgentPipeSessionEffectsFactory: Clone + Send {
    type Effects: NativeHostSideEffects;

    fn create_effects(&self) -> Self::Effects;
}

impl<F, E> AgentPipeSessionEffectsFactory for F
where
    F: Fn() -> E + Clone + Send,
    E: NativeHostSideEffects,
{
    type Effects = E;

    fn create_effects(&self) -> Self::Effects {
        self()
    }
}
