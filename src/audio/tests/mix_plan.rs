use super::{available_microphone, available_system, input_spec, mono_format, target_format};
use crate::audio::{
    devices::AudioSourceMode,
    loopback::system_loopback_input_spec,
    mixer::{AudioInputKind, MixPlan, MixPlanError, plan_mix},
    wasapi_capture::microphone_input_spec,
};

#[test]
fn mixed_audio_plan_marks_resample_channel_remap_and_drift_correction() -> Result<(), &'static str>
{
    let Ok(microphone) = microphone_input_spec(
        &available_microphone("mic-default", true),
        crate::audio::mixer::PcmFormat {
            sample_rate_hz: 44_100,
            channels: 1,
        },
        35,
    ) else {
        return Err("available microphone should narrow");
    };
    let Ok(system) = system_loopback_input_spec(&available_system(), target_format(), 0) else {
        return Err("available loopback should narrow");
    };

    let Ok(plan) = plan_mix(
        AudioSourceMode::Mixed,
        Some(microphone),
        Some(system),
        target_format(),
    ) else {
        return Err("valid mixed plan");
    };

    let Some(microphone_plan) = plan.microphone else {
        return Err("microphone plan");
    };
    assert_eq!(microphone_plan.kind, AudioInputKind::Microphone);
    assert!(microphone_plan.resample_required);
    assert!(microphone_plan.channel_remap_required);
    assert!(microphone_plan.drift_correction_required);
    let Some(system_plan) = plan.system_loopback else {
        return Err("system plan");
    };
    assert_eq!(system_plan.kind, AudioInputKind::SystemLoopback);
    Ok(())
}

#[test]
fn mix_plan_fails_closed_for_missing_sources_or_excessive_drift() -> Result<(), &'static str> {
    assert!(matches!(
        plan_mix(AudioSourceMode::System, None, None, target_format()),
        Err(MixPlanError::MissingSystemAudio)
    ));

    let Ok(microphone) = microphone_input_spec(
        &available_microphone("mic-default", true),
        target_format(),
        700,
    ) else {
        return Err("input narrowing does not validate drift budget");
    };
    assert!(matches!(
        plan_mix(
            AudioSourceMode::Microphone,
            Some(microphone),
            None,
            target_format()
        ),
        Err(MixPlanError::ExcessiveClockDrift)
    ));
    Ok(())
}

pub(super) fn mixed_plan() -> Result<MixPlan, MixPlanError> {
    plan_mix(
        AudioSourceMode::Mixed,
        Some(input_spec(AudioInputKind::Microphone, mono_format(), 0)),
        Some(input_spec(
            AudioInputKind::SystemLoopback,
            target_format(),
            0,
        )),
        target_format(),
    )
}
