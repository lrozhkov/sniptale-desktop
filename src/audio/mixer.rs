//! PCM mix, resample, and sync owner.
use super::devices::AudioSourceMode;

const DRIFT_CORRECTION_THRESHOLD_PPM: i32 = 20;
const MAX_CORRECTABLE_DRIFT_PPM: i32 = 500;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AudioInputKind {
    Microphone,
    SystemLoopback,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MixPlanError {
    InvalidFormat,
    MissingMicrophone,
    MissingSystemAudio,
    ExcessiveClockDrift,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PcmFormat {
    pub(crate) sample_rate_hz: u32,
    pub(crate) channels: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct AudioInputSpec {
    pub(crate) kind: AudioInputKind,
    pub(crate) format: PcmFormat,
    pub(crate) clock_drift_ppm: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InputMixPlan {
    pub(crate) kind: AudioInputKind,
    pub(crate) resample_required: bool,
    pub(crate) channel_remap_required: bool,
    pub(crate) drift_correction_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MixPlan {
    pub(crate) target_format: PcmFormat,
    pub(crate) microphone: Option<InputMixPlan>,
    pub(crate) system_loopback: Option<InputMixPlan>,
}

pub(crate) fn plan_mix(
    mode: AudioSourceMode,
    microphone: Option<AudioInputSpec>,
    system_loopback: Option<AudioInputSpec>,
    target_format: PcmFormat,
) -> Result<MixPlan, MixPlanError> {
    validate_format(target_format)?;

    let microphone = match mode {
        AudioSourceMode::Microphone | AudioSourceMode::Mixed => {
            Some(input_plan(required_microphone(microphone)?, target_format)?)
        }
        AudioSourceMode::System => None,
    };
    let system_loopback = match mode {
        AudioSourceMode::System | AudioSourceMode::Mixed => Some(input_plan(
            required_system(system_loopback)?,
            target_format,
        )?),
        AudioSourceMode::Microphone => None,
    };

    Ok(MixPlan {
        target_format,
        microphone,
        system_loopback,
    })
}

fn required_microphone(input: Option<AudioInputSpec>) -> Result<AudioInputSpec, MixPlanError> {
    match input {
        Some(input) if input.kind == AudioInputKind::Microphone => Ok(input),
        _ => Err(MixPlanError::MissingMicrophone),
    }
}

fn required_system(input: Option<AudioInputSpec>) -> Result<AudioInputSpec, MixPlanError> {
    match input {
        Some(input) if input.kind == AudioInputKind::SystemLoopback => Ok(input),
        _ => Err(MixPlanError::MissingSystemAudio),
    }
}

fn input_plan(
    input: AudioInputSpec,
    target_format: PcmFormat,
) -> Result<InputMixPlan, MixPlanError> {
    validate_format(input.format)?;
    let abs_drift = input.clock_drift_ppm.abs();
    if abs_drift > MAX_CORRECTABLE_DRIFT_PPM {
        return Err(MixPlanError::ExcessiveClockDrift);
    }

    Ok(InputMixPlan {
        kind: input.kind,
        resample_required: input.format.sample_rate_hz != target_format.sample_rate_hz,
        channel_remap_required: input.format.channels != target_format.channels,
        drift_correction_required: abs_drift >= DRIFT_CORRECTION_THRESHOLD_PPM,
    })
}

pub(super) fn validate_format(format: PcmFormat) -> Result<(), MixPlanError> {
    if !(8_000..=192_000).contains(&format.sample_rate_hz) || !(1..=8).contains(&format.channels) {
        return Err(MixPlanError::InvalidFormat);
    }
    Ok(())
}
