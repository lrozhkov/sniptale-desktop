use super::{
    FakePcmSource, decode_i16, input_spec, mix_plan::mixed_plan, mono_format, owned_chunk,
    target_format,
};
use crate::audio::{
    devices::AudioSourceMode,
    mixer::{AudioInputKind, plan_mix},
    pcm_source::{MixedPcmPoll, MixedPcmSource, PcmMixPipeline, PcmSourceError, PcmSourcePoll},
};

#[test]
fn pcm_mix_pipeline_mixes_synced_sources_until_drained() -> Result<(), &'static str> {
    let plan = mixed_plan().map_err(|_| "valid mixed plan")?;
    let microphone = FakePcmSource::new(vec![
        Ok(PcmSourcePoll::Chunk(owned_chunk(
            AudioInputKind::Microphone,
            mono_format(),
            &[1_000],
            0,
            20,
        ))),
        Ok(PcmSourcePoll::Drained),
    ]);
    let system = FakePcmSource::new(vec![
        Ok(PcmSourcePoll::Chunk(owned_chunk(
            AudioInputKind::SystemLoopback,
            target_format(),
            &[3_000, 1_000],
            0,
            20,
        ))),
        Ok(PcmSourcePoll::Drained),
    ]);
    let mut pipeline = PcmMixPipeline::new(plan, Some(microphone), Some(system));

    let MixedPcmPoll::Chunk(mixed) = pipeline
        .next_mixed_chunk()
        .map_err(|_| "first mixed chunk")?
    else {
        return Err("first mixed chunk should be present");
    };
    assert_eq!(decode_i16(&mixed.bytes), vec![2_000, 1_000]);
    assert_eq!(
        pipeline.next_mixed_chunk().map_err(|_| "drain")?,
        MixedPcmPoll::Drained
    );
    assert_eq!(
        pipeline.next_mixed_chunk().map_err(|_| "stable drain")?,
        MixedPcmPoll::Drained
    );

    let (microphone, system) = pipeline.into_parts();
    let Some(microphone) = microphone else {
        return Err("microphone source");
    };
    let Some(system) = system else {
        return Err("system source");
    };
    assert_eq!(microphone.polls, 2);
    assert_eq!(system.polls, 2);
    Ok(())
}

#[test]
fn pcm_mix_pipeline_keeps_pending_sources_pollable() -> Result<(), &'static str> {
    let plan = plan_mix(
        AudioSourceMode::Microphone,
        Some(input_spec(AudioInputKind::Microphone, mono_format(), 0)),
        None,
        target_format(),
    )
    .map_err(|_| "valid microphone plan")?;
    let microphone = FakePcmSource::new(vec![
        Ok(PcmSourcePoll::Pending),
        Ok(PcmSourcePoll::Chunk(owned_chunk(
            AudioInputKind::Microphone,
            mono_format(),
            &[2_000],
            0,
            20,
        ))),
        Ok(PcmSourcePoll::Drained),
    ]);
    let mut pipeline = PcmMixPipeline::new(plan, Some(microphone), None::<FakePcmSource>);

    assert_eq!(
        pipeline.next_mixed_chunk().map_err(|_| "pending")?,
        MixedPcmPoll::Pending
    );
    let MixedPcmPoll::Chunk(mixed) = pipeline.next_mixed_chunk().map_err(|_| "chunk")? else {
        return Err("chunk after pending");
    };
    assert_eq!(decode_i16(&mixed.bytes), vec![2_000, 2_000]);
    assert_eq!(
        pipeline.next_mixed_chunk().map_err(|_| "drain")?,
        MixedPcmPoll::Drained
    );
    Ok(())
}

#[test]
fn pcm_mix_pipeline_fails_when_mixed_source_ends_early() -> Result<(), &'static str> {
    let plan = mixed_plan().map_err(|_| "valid mixed plan")?;
    let microphone = FakePcmSource::new(vec![Ok(PcmSourcePoll::Chunk(owned_chunk(
        AudioInputKind::Microphone,
        mono_format(),
        &[1_000],
        0,
        20,
    )))]);
    let system = FakePcmSource::new(vec![Ok(PcmSourcePoll::Drained)]);
    let mut pipeline = PcmMixPipeline::new(plan, Some(microphone), Some(system));

    assert!(matches!(
        pipeline.next_mixed_chunk(),
        Err(PcmSourceError::SourceEndedEarly(
            AudioInputKind::SystemLoopback
        ))
    ));
    Ok(())
}

#[test]
fn pcm_mix_pipeline_propagates_source_error() -> Result<(), &'static str> {
    let plan = plan_mix(
        AudioSourceMode::Microphone,
        Some(input_spec(AudioInputKind::Microphone, mono_format(), 0)),
        None,
        target_format(),
    )
    .map_err(|_| "valid microphone plan")?;
    let microphone = FakePcmSource::new(vec![Err(PcmSourceError::Source(
        AudioInputKind::Microphone,
    ))]);
    let mut pipeline = PcmMixPipeline::new(plan, Some(microphone), None::<FakePcmSource>);

    assert!(matches!(
        pipeline.next_mixed_chunk(),
        Err(PcmSourceError::Source(AudioInputKind::Microphone))
    ));
    Ok(())
}
