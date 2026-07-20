use super::*;

#[test]
fn encoder_plan_uses_requested_playable_mp4_h264_aac_settings() -> Result<(), EncoderPlanError> {
    let plan = plan_encoder(encoder_request(), encoder_capabilities())?;

    assert_eq!(plan.encoder, EncoderKind::Hardware);
    assert_eq!(plan.width, 1920);
    assert_eq!(plan.height, 1080);
    assert_eq!(plan.audio_mode, EncoderAudioMode::Mixed);
    Ok(())
}

#[test]
fn encoder_plan_rejects_unplayable_or_unsupported_settings() {
    let mut request = encoder_request();
    request.audio_bitrate_kbps = 100;
    assert_eq!(
        plan_encoder(request, encoder_capabilities()),
        Err(EncoderPlanError::InvalidAudioBitrate)
    );

    request = encoder_request();
    request.width = 7680;
    assert_eq!(
        plan_encoder(request, encoder_capabilities()),
        Err(EncoderPlanError::InvalidGeometry)
    );

    let mut capabilities = encoder_capabilities();
    capabilities.hardware_available = false;
    capabilities.software_available = false;
    assert_eq!(
        plan_encoder(encoder_request(), capabilities),
        Err(EncoderPlanError::EncoderUnavailable)
    );
}

#[test]
fn quality_report_selects_browser_playable_h264_profile_and_level() -> Result<(), EncoderPlanError>
{
    let plan = plan_encoder(encoder_request(), encoder_capabilities())?;
    let report = quality_report(plan)?;

    assert_eq!(report.encoder, EncoderKind::Hardware);
    assert_eq!(report.h264_profile, H264Profile::Main);
    assert_eq!(report.h264_level, H264Level::L4_0);
    assert_eq!(report.audio_bitrate_kbps, 160);
    assert_eq!(report.fps, 30);
    Ok(())
}

#[test]
fn quality_report_uses_high_profile_and_level_for_large_sources() -> Result<(), EncoderPlanError> {
    let mut request = encoder_request();
    request.width = 3840;
    request.height = 2160;
    request.fps = 60;
    request.video_bitrate_mbps = 60.0;

    let report = quality_report(plan_encoder(request, encoder_capabilities())?)?;

    assert_eq!(report.h264_profile, H264Profile::High);
    assert_eq!(report.h264_level, H264Level::L5_2);
    Ok(())
}

#[test]
fn h264_level_uses_macroblock_limits_for_1440p_and_high_fps_sources() -> Result<(), EncoderPlanError>
{
    let mut request = encoder_request();
    request.width = 2560;
    request.height = 1440;
    request.fps = 30;
    assert_eq!(
        quality_report(plan_encoder(request, encoder_capabilities())?)?.h264_level,
        H264Level::L5_0
    );

    request = encoder_request();
    request.fps = 60;
    assert_eq!(
        quality_report(plan_encoder(request, encoder_capabilities())?)?.h264_level,
        H264Level::L4_2
    );
    Ok(())
}

#[test]
fn h264_level_accepts_level_5_2_boundaries_and_rejects_larger_plans() {
    let capabilities = EncoderCapabilities {
        max_width: 7680,
        max_height: 4320,
        max_fps: 60,
        hardware_available: true,
        software_available: true,
    };

    let mut request = encoder_request();
    request.width = 4096;
    request.height = 2304;
    assert_eq!(
        plan_encoder(request, capabilities).and_then(h264_level_for_plan),
        Ok(H264Level::L5_2)
    );

    request.width = 3840;
    request.height = 2304;
    request.fps = 60;
    assert_eq!(
        plan_encoder(request, capabilities).and_then(h264_level_for_plan),
        Ok(H264Level::L5_2)
    );

    request.width = 4112;
    request.height = 2304;
    request.fps = 30;
    assert_eq!(
        plan_encoder(request, capabilities),
        Err(EncoderPlanError::UnsupportedH264Level)
    );

    request.width = 3840;
    request.height = 2320;
    request.fps = 60;
    assert_eq!(
        plan_encoder(request, capabilities),
        Err(EncoderPlanError::UnsupportedH264Level)
    );

    request.width = 7680;
    request.height = 4320;
    assert_eq!(
        plan_encoder(request, capabilities),
        Err(EncoderPlanError::UnsupportedH264Level)
    );
}

#[test]
fn mp4_sink_writer_plan_uses_h264_aac_browser_playable_stream_settings() -> Result<(), String> {
    let plan = plan_encoder(encoder_request(), encoder_capabilities())
        .map_err(|error| format!("{error:?}"))?;
    let mix = mixed_audio_plan().map_err(|error| format!("{error:?}"))?;
    let writer = build_mp4_sink_writer_plan(plan, &mix).map_err(|error| format!("{error:?}"))?;

    assert_eq!(writer.video.width, 1920);
    assert_eq!(writer.video.height, 1080);
    assert_eq!(writer.video.fps_numerator, 30);
    assert_eq!(writer.video.fps_denominator, 1);
    assert_eq!(writer.video.frame_duration_100ns, 333_333);
    assert_eq!(writer.video.average_bitrate_bps, 12_000_000);
    assert_eq!(writer.video.top_down_stride_bytes, 7_680);
    assert_eq!(writer.video.profile, H264Profile::Main);
    assert_eq!(writer.video.level, H264Level::L4_0);
    assert_eq!(writer.audio.sample_rate_hz, 48_000);
    assert_eq!(writer.audio.channels, 2);
    assert_eq!(writer.audio.average_bitrate_bps, 160_000);
    assert_eq!(writer.audio.average_bytes_per_second, 20_000);
    Ok(())
}

#[test]
fn mp4_sink_writer_plan_rejects_aac_incompatible_target_format() -> Result<(), String> {
    let plan = plan_encoder(encoder_request(), encoder_capabilities())
        .map_err(|error| format!("{error:?}"))?;
    let mut mix = mixed_audio_plan().map_err(|error| format!("{error:?}"))?;
    mix.target_format = PcmFormat {
        sample_rate_hz: 22_050,
        channels: 2,
    };

    assert_eq!(
        build_mp4_sink_writer_plan(plan, &mix),
        Err(SinkWriterPlanError::AudioFormat)
    );

    mix.target_format = PcmFormat {
        sample_rate_hz: 48_000,
        channels: 6,
    };
    assert_eq!(
        build_mp4_sink_writer_plan(plan, &mix),
        Err(SinkWriterPlanError::AudioFormat)
    );
    Ok(())
}

fn sample_video_plan() -> H264SinkWriterStreamPlan {
    H264SinkWriterStreamPlan {
        width: 4,
        height: 2,
        fps_numerator: 30,
        fps_denominator: 1,
        frame_duration_100ns: 333_333,
        average_bitrate_bps: 2_000_000,
        top_down_stride_bytes: 16,
        profile: H264Profile::Main,
        level: H264Level::L4_0,
    }
}

fn sample_audio_plan() -> AacSinkWriterStreamPlan {
    AacSinkWriterStreamPlan {
        sample_rate_hz: 48_000,
        channels: 2,
        average_bitrate_bps: 160_000,
        average_bytes_per_second: 20_000,
    }
}

#[test]
fn sink_writer_sample_validation_accepts_padded_bgra_and_aligned_pcm() -> Result<(), String> {
    let video = sample_video_plan();
    let pixels = vec![0_u8; 40];
    let layout = validate_bgra_video_sample(
        &video,
        BgraVideoSample {
            width: 4,
            height: 2,
            stride: 20,
            bgra_bytes: &pixels,
            timestamp_100ns: 0,
            duration_100ns: 333_333,
        },
    )
    .map_err(|error| format!("{error:?}"))?;

    assert_eq!(layout.row_bytes, 16);
    assert_eq!(layout.source_span, 40);
    assert_eq!(layout.packed_len, 32);

    let audio = sample_audio_plan();
    let block_alignment = validate_pcm_audio_sample(
        &audio,
        PcmAudioSample {
            bytes: &[0_u8; 8],
            timestamp_100ns: 0,
            duration_100ns: 100_000,
        },
    )
    .map_err(|error| format!("{error:?}"))?;
    assert_eq!(block_alignment, 4);
    Ok(())
}

#[test]
fn sink_writer_sample_validation_rejects_invalid_timing_geometry_and_pcm_alignment() {
    let video = sample_video_plan();
    let pixels = vec![0_u8; 40];
    assert_eq!(
        validate_bgra_video_sample(
            &video,
            BgraVideoSample {
                width: 4,
                height: 2,
                stride: 20,
                bgra_bytes: &pixels,
                timestamp_100ns: -1,
                duration_100ns: 333_333,
            },
        ),
        Err(SinkWriterSampleError::InvalidTime)
    );
    assert_eq!(
        validate_bgra_video_sample(
            &video,
            BgraVideoSample {
                width: 5,
                height: 2,
                stride: 20,
                bgra_bytes: &pixels,
                timestamp_100ns: 0,
                duration_100ns: 333_333,
            },
        ),
        Err(SinkWriterSampleError::InvalidVideoGeometry)
    );

    let audio = sample_audio_plan();
    assert_eq!(
        validate_pcm_audio_sample(
            &audio,
            PcmAudioSample {
                bytes: &[0_u8; 3],
                timestamp_100ns: 0,
                duration_100ns: 100_000,
            },
        ),
        Err(SinkWriterSampleError::InvalidAudioBytes)
    );
}
