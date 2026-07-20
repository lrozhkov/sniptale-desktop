//! Recording runtime preflight facts and error mapping.
use crate::{
    audio::AudioEndpoint,
    recording::{
        encoder::EncoderCapabilities,
        quality::{RecordingQualityError, RecordingQualityLimits},
        runtime::{
            RecordingRuntimeError,
            plan::{
                RecordingAudioFacts, RecordingRuntimePlan, RecordingRuntimePlanError,
                RecordingSourceGeometry, plan_recording_runtime,
            },
        },
    },
};

use crate::recording::RecordingStartRequest;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecordingPreflightFacts {
    pub(crate) source: RecordingSourceGeometry,
    pub(crate) audio_endpoints: Vec<AudioEndpoint>,
    pub(crate) audio_facts: RecordingAudioFacts,
    pub(crate) encoder_capabilities: EncoderCapabilities,
    pub(crate) quality_limits: RecordingQualityLimits,
}

pub(crate) fn preflight_recording_start(
    request: &RecordingStartRequest,
    facts: &RecordingPreflightFacts,
) -> Result<RecordingRuntimePlan, RecordingRuntimeError> {
    plan_recording_runtime(
        request,
        facts.source,
        &facts.audio_endpoints,
        facts.audio_facts,
        facts.encoder_capabilities,
        facts.quality_limits,
    )
    .map_err(preflight_plan_error)
}

fn preflight_plan_error(error: RecordingRuntimePlanError) -> RecordingRuntimeError {
    match error {
        RecordingRuntimePlanError::Audio(_)
        | RecordingRuntimePlanError::Mix(_)
        | RecordingRuntimePlanError::Encoder(_)
        | RecordingRuntimePlanError::Quality(
            RecordingQualityError::InvalidSourceGeometry
            | RecordingQualityError::SourceExceedsCapability
            | RecordingQualityError::InvalidManualBitrate,
        ) => RecordingRuntimeError::UnsupportedMode,
        RecordingRuntimePlanError::Quality(RecordingQualityError::InvalidNumber)
        | RecordingRuntimePlanError::InvalidEffectiveQuality => {
            RecordingRuntimeError::RuntimeFailed
        }
    }
}
