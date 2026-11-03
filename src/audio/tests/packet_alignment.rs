use super::{FakePcmSource, decode_i16, input_spec, owned_chunk, target_format};
use crate::audio::{
    devices::AudioSourceMode,
    mixer::{AudioInputKind, MixPlan, plan_mix},
    pcm_mix::PcmMixError,
    pcm_source::{MixedPcmPoll, MixedPcmSource, PcmMixPipeline, PcmSourceError, PcmSourcePoll},
};

fn plan() -> Result<MixPlan, &'static str> {
    plan_mix(
        AudioSourceMode::Mixed,
        Some(input_spec(AudioInputKind::Microphone, target_format(), 0)),
        Some(input_spec(
            AudioInputKind::SystemLoopback,
            target_format(),
            0,
        )),
        target_format(),
    )
    .map_err(|_| "mixed plan")
}

fn packets(
    kind: AudioInputKind,
    values: &[i16],
    size: usize,
    pending: bool,
) -> Vec<Result<PcmSourcePoll, PcmSourceError>> {
    let mut result = Vec::new();
    let mut offset = 0_i64;
    for part in values.chunks(size) {
        if pending {
            result.push(Ok(PcmSourcePoll::Pending));
        }
        let frames = i64::try_from(part.len()).unwrap_or(0);
        let start = offset * 10_000_000 / 48_000;
        let end = (offset + frames) * 10_000_000 / 48_000;
        let stereo: Vec<_> = part.iter().flat_map(|sample| [*sample, *sample]).collect();
        result.push(Ok(PcmSourcePoll::Chunk(owned_chunk(
            kind,
            target_format(),
            &stereo,
            start,
            end - start,
        ))));
        offset += frames;
    }
    result.push(Ok(PcmSourcePoll::Drained));
    result
}

#[test]
fn ready_packet_survives_a_pending_peer_without_another_source_poll() -> Result<(), &'static str> {
    let microphone = FakePcmSource::new(packets(AudioInputKind::Microphone, &[100], 1, false));
    let system = FakePcmSource::new(packets(AudioInputKind::SystemLoopback, &[300], 1, true));
    let mut pipeline = PcmMixPipeline::new(plan()?, Some(microphone), Some(system));
    assert_eq!(
        pipeline.next_mixed_chunk().map_err(|_| "pending")?,
        MixedPcmPoll::Pending
    );
    let MixedPcmPoll::Chunk(chunk) = pipeline.next_mixed_chunk().map_err(|_| "ready")? else {
        return Err("ready packet lost");
    };
    assert_eq!(decode_i16(&chunk.bytes), vec![200, 200]);
    let (microphone, system) = pipeline.into_parts();
    assert_eq!(microphone.ok_or("microphone")?.polls, 1);
    assert_eq!(system.ok_or("system")?.polls, 2);
    Ok(())
}

#[test]
fn independent_packet_partitions_preserve_all_samples_and_cumulative_time()
-> Result<(), &'static str> {
    let microphone: Vec<i16> = (100..116).collect();
    let system: Vec<i16> = (1_000..1_016).collect();
    let expected: Vec<i16> = (550..566).flat_map(|sample| [sample, sample]).collect();
    for microphone_size in 1..=7 {
        for system_size in 1..=7 {
            let microphone = FakePcmSource::new(packets(
                AudioInputKind::Microphone,
                &microphone,
                microphone_size,
                true,
            ));
            let system = FakePcmSource::new(packets(
                AudioInputKind::SystemLoopback,
                &system,
                system_size,
                true,
            ));
            let mut pipeline = PcmMixPipeline::new(plan()?, Some(microphone), Some(system));
            let mut decoded = Vec::new();
            let mut end = 0;
            let mut drained = false;
            for _ in 0..128 {
                match pipeline.next_mixed_chunk().map_err(|_| "partitioned mix")? {
                    MixedPcmPoll::Pending => {}
                    MixedPcmPoll::Drained => {
                        drained = true;
                        break;
                    }
                    MixedPcmPoll::Chunk(chunk) => {
                        assert_eq!(chunk.timestamp_100ns, end);
                        assert!(chunk.duration_100ns > 0);
                        end += chunk.duration_100ns;
                        decoded.extend(decode_i16(&chunk.bytes));
                    }
                }
            }
            assert!(drained);
            assert_eq!(decoded, expected);
            assert_eq!(end, 16 * 10_000_000 / 48_000);
            assert_eq!(
                pipeline.next_mixed_chunk().map_err(|_| "stable drain")?,
                MixedPcmPoll::Drained
            );
        }
    }
    Ok(())
}

#[test]
fn stale_packet_and_different_start_epochs_fail_closed() -> Result<(), &'static str> {
    let mut stale = packets(AudioInputKind::Microphone, &[100, 100], 1, false);
    if let Ok(PcmSourcePoll::Chunk(chunk)) = &mut stale[1] {
        chunk.timestamp_100ns = 0;
    }
    let system = packets(AudioInputKind::SystemLoopback, &[300, 300], 1, false);
    let mut pipeline = PcmMixPipeline::new(
        plan()?,
        Some(FakePcmSource::new(stale)),
        Some(FakePcmSource::new(system)),
    );
    assert!(matches!(
        pipeline.next_mixed_chunk(),
        Ok(MixedPcmPoll::Chunk(_))
    ));
    assert_eq!(
        pipeline.next_mixed_chunk(),
        Err(PcmSourceError::Mix(PcmMixError::UnsyncedInputs))
    );
    let microphone = packets(AudioInputKind::Microphone, &[100], 1, false);
    let mut system = packets(AudioInputKind::SystemLoopback, &[300], 1, false);
    if let Ok(PcmSourcePoll::Chunk(chunk)) = &mut system[0] {
        chunk.timestamp_100ns = 1;
    }
    let mut pipeline = PcmMixPipeline::new(
        plan()?,
        Some(FakePcmSource::new(microphone)),
        Some(FakePcmSource::new(system)),
    );
    assert_eq!(
        pipeline.next_mixed_chunk(),
        Err(PcmSourceError::Mix(PcmMixError::UnsyncedInputs))
    );
    Ok(())
}

#[test]
fn oversized_packet_is_rejected_before_resampling() -> Result<(), &'static str> {
    let oversized = vec![0; 48_000 * 2 * 2 + 2];
    let microphone = FakePcmSource::new(vec![Ok(PcmSourcePoll::Chunk(owned_chunk(
        AudioInputKind::Microphone,
        target_format(),
        &oversized,
        0,
        20_000_001,
    )))]);
    let system = FakePcmSource::new(packets(AudioInputKind::SystemLoopback, &[300], 1, false));
    let mut pipeline = PcmMixPipeline::new(plan()?, Some(microphone), Some(system));
    assert_eq!(
        pipeline.next_mixed_chunk(),
        Err(PcmSourceError::Mix(PcmMixError::OutputTooLarge))
    );
    Ok(())
}

#[test]
fn mixed_packets_resample_each_input_before_matching_frame_prefixes() -> Result<(), &'static str> {
    let mono = crate::audio::mixer::PcmFormat {
        sample_rate_hz: 24_000,
        channels: 1,
    };
    let plan = plan_mix(
        AudioSourceMode::Mixed,
        Some(input_spec(AudioInputKind::Microphone, mono, 0)),
        Some(input_spec(
            AudioInputKind::SystemLoopback,
            target_format(),
            0,
        )),
        target_format(),
    )
    .map_err(|_| "resampling plan")?;
    let microphone = FakePcmSource::new(vec![
        Ok(PcmSourcePoll::Chunk(owned_chunk(
            AudioInputKind::Microphone,
            mono,
            &[100, 200],
            0,
            833,
        ))),
        Ok(PcmSourcePoll::Drained),
    ]);
    let system = FakePcmSource::new(packets(
        AudioInputKind::SystemLoopback,
        &[300, 400, 500, 600],
        3,
        false,
    ));
    let mut pipeline = PcmMixPipeline::new(plan, Some(microphone), Some(system));
    let mut decoded = Vec::new();
    for _ in 0..2 {
        let MixedPcmPoll::Chunk(chunk) = pipeline
            .next_mixed_chunk()
            .map_err(|_| "resampled prefix")?
        else {
            return Err("missing prefix");
        };
        decoded.extend(decode_i16(&chunk.bytes));
    }
    assert_eq!(decoded, vec![200, 200, 250, 250, 350, 350, 400, 400]);
    assert_eq!(
        pipeline.next_mixed_chunk().map_err(|_| "drain")?,
        MixedPcmPoll::Drained
    );
    Ok(())
}

#[test]
fn leftover_packet_cannot_be_silently_truncated_when_peer_ends() -> Result<(), &'static str> {
    let microphone = FakePcmSource::new(packets(AudioInputKind::Microphone, &[100; 4], 4, false));
    let system = FakePcmSource::new(packets(AudioInputKind::SystemLoopback, &[300; 3], 3, false));
    let mut pipeline = PcmMixPipeline::new(plan()?, Some(microphone), Some(system));
    assert!(matches!(
        pipeline.next_mixed_chunk(),
        Ok(MixedPcmPoll::Chunk(_))
    ));
    assert_eq!(
        pipeline.next_mixed_chunk(),
        Err(PcmSourceError::SourceEndedEarly(
            AudioInputKind::SystemLoopback
        ))
    );
    Ok(())
}

#[test]
fn normalized_time_overflow_and_invalid_source_formats_are_rejected() -> Result<(), &'static str> {
    for (format, epoch, expected) in [
        (target_format(), i64::MAX - 1, PcmMixError::InvalidInputTime),
        (
            crate::audio::mixer::PcmFormat {
                sample_rate_hz: 0,
                channels: 2,
            },
            0,
            PcmMixError::InvalidFormat,
        ),
    ] {
        let microphone = FakePcmSource::new(vec![Ok(PcmSourcePoll::Chunk(owned_chunk(
            AudioInputKind::Microphone,
            format,
            &[100, 100],
            epoch,
            1,
        )))]);
        let system = FakePcmSource::new(vec![Ok(PcmSourcePoll::Chunk(owned_chunk(
            AudioInputKind::SystemLoopback,
            target_format(),
            &[300, 300],
            epoch,
            1,
        )))]);
        let mut pipeline = PcmMixPipeline::new(plan()?, Some(microphone), Some(system));
        assert_eq!(
            pipeline.next_mixed_chunk(),
            Err(PcmSourceError::Mix(expected))
        );
    }
    Ok(())
}
