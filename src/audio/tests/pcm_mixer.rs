use super::{decode_i16, input_spec, mix_plan::mixed_plan, mono_format, pcm_bytes, target_format};
use crate::audio::{
    devices::AudioSourceMode,
    mixer::{AudioInputKind, plan_mix},
    pcm_mix::{PcmInputChunk, PcmMixError, mix_pcm_chunks},
};

#[test]
fn pcm_mixer_remaps_mono_microphone_to_stereo() -> Result<(), &'static str> {
    let plan = plan_mix(
        AudioSourceMode::Microphone,
        Some(input_spec(AudioInputKind::Microphone, mono_format(), 0)),
        None,
        target_format(),
    )
    .map_err(|_| "valid microphone mix plan")?;

    let microphone_bytes = pcm_bytes(&[1_000, -1_000]);
    let mixed = mix_pcm_chunks(
        &plan,
        Some(input_chunk(
            AudioInputKind::Microphone,
            mono_format(),
            &microphone_bytes,
            10,
            20,
        )),
        None,
    )
    .map_err(|_| "mono microphone chunk should mix")?;

    assert_eq!(mixed.timestamp_100ns, 10);
    assert_eq!(mixed.duration_100ns, 20);
    assert_eq!(decode_i16(&mixed.bytes), vec![1_000, 1_000, -1_000, -1_000]);
    Ok(())
}

#[test]
fn pcm_mixer_resamples_system_loopback_to_target_rate() -> Result<(), &'static str> {
    let source_format = crate::audio::mixer::PcmFormat {
        sample_rate_hz: 24_000,
        channels: 2,
    };
    let plan = plan_mix(
        AudioSourceMode::System,
        None,
        Some(input_spec(AudioInputKind::SystemLoopback, source_format, 0)),
        target_format(),
    )
    .map_err(|_| "valid system mix plan")?;

    let system_bytes = pcm_bytes(&[100, 200, 300, 400]);
    let mixed = mix_pcm_chunks(
        &plan,
        None,
        Some(input_chunk(
            AudioInputKind::SystemLoopback,
            source_format,
            &system_bytes,
            0,
            40,
        )),
    )
    .map_err(|_| "system chunk should resample")?;

    assert_eq!(
        decode_i16(&mixed.bytes),
        vec![100, 200, 100, 200, 300, 400, 300, 400]
    );
    Ok(())
}

#[test]
fn pcm_mixer_keeps_short_downsampled_chunks_nonempty() -> Result<(), &'static str> {
    let source_format = crate::audio::mixer::PcmFormat {
        sample_rate_hz: 192_000,
        channels: 1,
    };
    let plan = plan_mix(
        AudioSourceMode::Microphone,
        Some(input_spec(AudioInputKind::Microphone, source_format, 0)),
        None,
        target_format(),
    )
    .map_err(|_| "valid microphone mix plan")?;

    let microphone_bytes = pcm_bytes(&[7_000]);
    let mixed = mix_pcm_chunks(
        &plan,
        Some(input_chunk(
            AudioInputKind::Microphone,
            source_format,
            &microphone_bytes,
            0,
            10,
        )),
        None,
    )
    .map_err(|_| "short downsampled chunk should remain nonempty")?;

    assert_eq!(decode_i16(&mixed.bytes), vec![7_000, 7_000]);
    Ok(())
}

#[test]
fn pcm_mixer_averages_synced_microphone_and_system_audio() -> Result<(), &'static str> {
    let plan = mixed_plan().map_err(|_| "valid mixed plan")?;

    let microphone_bytes = pcm_bytes(&[1_000, -1_000]);
    let system_bytes = pcm_bytes(&[3_000, 1_000, -3_000, -1_000]);
    let mixed = mix_pcm_chunks(
        &plan,
        Some(input_chunk(
            AudioInputKind::Microphone,
            mono_format(),
            &microphone_bytes,
            30,
            40,
        )),
        Some(input_chunk(
            AudioInputKind::SystemLoopback,
            target_format(),
            &system_bytes,
            30,
            40,
        )),
    )
    .map_err(|_| "synced mixed chunks should mix")?;

    assert_eq!(decode_i16(&mixed.bytes), vec![2_000, 1_000, -2_000, -1_000]);
    Ok(())
}

#[test]
fn pcm_mixer_fails_closed_for_missing_or_unsynced_sources() -> Result<(), &'static str> {
    let plan = mixed_plan().map_err(|_| "valid mixed plan")?;
    let microphone_bytes = pcm_bytes(&[1_000]);
    let system_bytes = pcm_bytes(&[1_000, 1_000]);
    assert!(matches!(
        mix_pcm_chunks(
            &plan,
            Some(input_chunk(
                AudioInputKind::Microphone,
                mono_format(),
                &microphone_bytes,
                0,
                20,
            )),
            None,
        ),
        Err(PcmMixError::MissingSystemAudio)
    ));
    assert!(matches!(
        mix_pcm_chunks(
            &plan,
            Some(input_chunk(
                AudioInputKind::Microphone,
                mono_format(),
                &microphone_bytes,
                0,
                20,
            )),
            Some(input_chunk(
                AudioInputKind::SystemLoopback,
                target_format(),
                &system_bytes,
                1,
                20,
            )),
        ),
        Err(PcmMixError::UnsyncedInputs)
    ));
    Ok(())
}

fn input_chunk(
    kind: AudioInputKind,
    format: crate::audio::mixer::PcmFormat,
    bytes: &[u8],
    timestamp_100ns: i64,
    duration_100ns: i64,
) -> PcmInputChunk<'_> {
    PcmInputChunk {
        kind,
        format,
        bytes,
        timestamp_100ns,
        duration_100ns,
    }
}
