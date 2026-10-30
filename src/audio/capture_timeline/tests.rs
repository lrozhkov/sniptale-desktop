use super::*;

#[test]
fn capture_timeline_preserves_a_device_gap_instead_of_shortening_audio()
-> Result<(), CaptureTimelineError> {
    let mut clock = CaptureTimeline::new(48_000, 48_000)?;
    clock.queue_packet(Some(0), 480)?;
    assert!(clock.next_emission()?.is_some_and(|packet| !packet.silence));
    // A loaded-guest MP4 lost approximately 4.555 seconds from its audio track.
    assert_eq!(clock.queue_packet(Some(219_120), 480)?, 218_640);
    let mut padding = 0;
    let mut end = 100_000;
    while let Some(packet) = clock.next_emission()? {
        assert_eq!(packet.timestamp_100ns, end);
        end += packet.duration_100ns;
        if packet.silence {
            assert!(packet.frames <= 480);
            padding += packet.frames;
        }
    }
    assert_eq!(padding, 218_640);
    assert_eq!(end, 45_750_000);
    clock.queue_packet(Some(219_600), 480)?;
    assert_eq!(
        clock.next_emission()?.map(|packet| packet.timestamp_100ns),
        Some(end)
    );
    Ok(())
}

#[test]
fn capture_timeline_emits_a_long_gap_lazily_in_bounded_steps() -> Result<(), CaptureTimelineError> {
    let mut clock = CaptureTimeline::new(48_000, 48_000)?;
    clock.queue_packet(Some(48_000 * 86_400), 480)?;
    for expected_start in [0, 100_000] {
        let packet = clock
            .next_emission()?
            .ok_or(CaptureTimelineError::InvalidPacket)?;
        assert!(packet.silence);
        assert_eq!(packet.frames, 480);
        assert_eq!(packet.timestamp_100ns, expected_start);
        assert_eq!(packet.duration_100ns, 100_000);
    }
    assert_eq!(
        clock.queue_packet(Some(0), 480),
        Err(CaptureTimelineError::PacketPending)
    );
    Ok(())
}

#[test]
fn capture_timeline_retains_initial_silence_and_fractional_frame_rounding()
-> Result<(), CaptureTimelineError> {
    let mut clock = CaptureTimeline::new(44_100, 44_100)?;
    clock.queue_packet(Some(44_100), 1)?;
    let mut end = 0;
    while let Some(packet) = clock.next_emission()? {
        assert_eq!(packet.timestamp_100ns, end);
        end += packet.duration_100ns;
    }
    for position in 44_101..44_200 {
        clock.queue_packet(Some(position), 1)?;
        let packet = clock
            .next_emission()?
            .ok_or(CaptureTimelineError::InvalidPacket)?;
        assert_eq!(packet.timestamp_100ns, end);
        assert!(!packet.silence);
        end += packet.duration_100ns;
    }
    assert_eq!(end, frame_time(44_200, 44_100)?);
    Ok(())
}

#[test]
fn capture_timeline_rejects_invalid_pending_and_overflow_packets()
-> Result<(), CaptureTimelineError> {
    assert!(matches!(
        CaptureTimeline::new(0, 48_000),
        Err(CaptureTimelineError::InvalidRate)
    ));
    let mut clock = CaptureTimeline::new(48_000, 48_000)?;
    assert_eq!(
        clock.queue_packet(Some(0), 0),
        Err(CaptureTimelineError::InvalidPacket)
    );
    assert_eq!(
        clock.queue_packet(Some(u64::MAX), 1),
        Err(CaptureTimelineError::Overflow)
    );
    assert_eq!(
        clock.queue_packet(Some(i64::MAX.unsigned_abs()), 1),
        Err(CaptureTimelineError::Overflow)
    );
    let mut converted = CaptureTimeline::new(192_000, 8_000)?;
    assert_eq!(
        converted.queue_packet(Some(u64::MAX), 1),
        Err(CaptureTimelineError::Overflow)
    );
    clock.queue_packet(Some(0), 480)?;
    assert_eq!(
        clock.queue_packet(Some(480), 1),
        Err(CaptureTimelineError::PacketPending)
    );
    clock.next_emission()?;
    assert_eq!(clock.queue_packet(Some(480), 480)?, 0);
    assert!(clock.next_emission()?.is_some_and(|packet| !packet.silence));
    assert_eq!(clock.next_emission()?, None);
    Ok(())
}

#[test]
fn capture_timeline_keeps_pcm_when_driver_positions_overlap_or_reset()
-> Result<(), CaptureTimelineError> {
    let mut clock = CaptureTimeline::new(48_000, 48_000)?;
    // Headers observed from shared 48 kHz stereo PCM with AUTOCONVERTPCM in Windows 11.
    // The first packet has 453 frames; subsequent silent packets repeat device position 0.
    let mut end = 0;
    let mut accepted_frames = 0;
    for (position, frames) in [(0, 453), (0, 488), (0, 488), (0, 488)] {
        assert_eq!(clock.queue_packet(Some(position), frames)?, 0);
        let emission = clock
            .next_emission()?
            .ok_or(CaptureTimelineError::InvalidPacket)?;
        assert!(!emission.silence);
        assert_eq!(emission.frames, frames);
        assert_eq!(emission.timestamp_100ns, end);
        accepted_frames += u64::from(frames);
        end += emission.duration_100ns;
        assert_eq!(end, frame_time(accepted_frames, 48_000)?);
        assert_eq!(clock.next_emission()?, None);
    }
    // After the rebase, an actual forward gap is still retained.
    assert_eq!(clock.queue_packet(Some(968), 480)?, 480);
    assert!(
        clock
            .next_emission()?
            .is_some_and(|emission| emission.silence)
    );
    assert!(
        clock
            .next_emission()?
            .is_some_and(|emission| !emission.silence)
    );
    Ok(())
}

#[test]
fn capture_timeline_uses_accepted_frames_for_uncertain_timestamps()
-> Result<(), CaptureTimelineError> {
    let mut clock = CaptureTimeline::new(48_000, 48_000)?;
    for expected_start in [0, 100_000, 200_000] {
        assert_eq!(clock.queue_packet(None, 480)?, 0);
        let emission = clock
            .next_emission()?
            .ok_or(CaptureTimelineError::InvalidPacket)?;
        assert!(!emission.silence);
        assert_eq!(emission.timestamp_100ns, expected_start);
        assert_eq!(emission.duration_100ns, 100_000);
    }
    assert_eq!(clock.queue_packet(Some(1_920), 480)?, 480);
    assert!(
        clock
            .next_emission()?
            .is_some_and(|emission| emission.silence)
    );
    Ok(())
}

#[test]
fn capture_timeline_converts_native_positions_before_detecting_gaps()
-> Result<(), CaptureTimelineError> {
    let mut clock = CaptureTimeline::new(48_000, 44_100)?;
    let mut end = 0;
    // The device advances 448 native frames while the converted stream returns 488 frames.
    for (position, frames) in [(0, 453), (448, 488), (896, 488)] {
        clock.queue_packet(Some(position), frames)?;
        while let Some(emission) = clock.next_emission()? {
            assert_eq!(emission.timestamp_100ns, end);
            end += emission.duration_100ns;
        }
    }
    assert_eq!(end, frame_time(1_463, 48_000)?);
    let mut invalid_rate = CaptureTimeline::new(48_000, 0);
    assert!(matches!(
        invalid_rate,
        Err(CaptureTimelineError::InvalidRate)
    ));
    invalid_rate = CaptureTimeline::new(0, 44_100);
    assert!(matches!(
        invalid_rate,
        Err(CaptureTimelineError::InvalidRate)
    ));
    Ok(())
}

#[test]
fn capture_timeline_handles_native_rates_above_the_owned_pcm_limit()
-> Result<(), CaptureTimelineError> {
    let mut clock = CaptureTimeline::new(48_000, 384_000)?;
    for (position, expected_time) in [(0, 0), (3_840, 100_000)] {
        assert_eq!(clock.queue_packet(Some(position), 480)?, 0);
        let emission = clock
            .next_emission()?
            .ok_or(CaptureTimelineError::InvalidPacket)?;
        assert!(!emission.silence);
        assert_eq!(emission.timestamp_100ns, expected_time);
        assert_eq!(emission.duration_100ns, 100_000);
    }
    Ok(())
}
