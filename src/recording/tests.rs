use std::{
    cell::RefCell,
    io::{Read, Seek, SeekFrom, Write},
    rc::Rc,
};

use super::disk_guard::{
    RecordingDiskGuardError, RecordingDiskSpace, ensure_recording_disk_headroom,
    recording_disk_reserve_bytes,
};
use super::encoder::{
    AacSinkWriterStreamPlan, EncoderAudioMode, EncoderCapabilities, EncoderKind, EncoderPlanError,
    EncoderRequest, H264Level, H264Profile, H264SinkWriterStreamPlan, SinkWriterPlanError,
    build_mp4_sink_writer_plan, h264_level_for_plan, plan_encoder,
    samples::{
        BgraVideoSample, PcmAudioSample, SinkWriterSampleError, validate_bgra_video_sample,
        validate_pcm_audio_sample,
    },
};
#[cfg(not(windows))]
use super::media_foundation_availability;
use super::quality::RecordingQualityLimits;
use super::quality::quality_report;
use super::recording_runtime_availability_from_facts;
use super::recovery::{RecordingRecoveryError, pending_recordings_for_reconnect};
use super::runtime::{
    FinalizedRecordingArtifact, RecordingRuntime, RecordingRuntimeError,
    RecordingRuntimeStartRequest, RecordingRuntimeStopRequest, StartedRecordingRuntime,
    UnsupportedRecordingRuntime, effective_quality_from_settings_policy,
    plan::{
        RecordingAudioFacts, RecordingRuntimePlanError, RecordingSourceGeometry,
        plan_recording_runtime,
    },
    preflight::{RecordingPreflightFacts, preflight_recording_start},
};
use super::service::{RecordingLifecycleService, TestRecordingMonotonicClock};
use super::session::{
    RecordingControl, RecordingSession, RecordingState, RecordingStateError, TerminalReason,
    TransitionOutcome,
};
use super::temp_media::{
    RecordingMediaMetadata, RecoveredTempMedia, StartupRecoveryDecision, TempMediaDiskStore,
    TempMediaRecord, TempMediaState, TempStoreDiskError, TempStoreError,
};
use super::transfer_store::{
    RecordingTransferStore, RecordingTransferStoreConfig, RecordingTransferStoreError,
};
#[cfg(not(windows))]
use super::video_encoder_availability;
use super::{MediaFoundationAvailability, VideoEncoderAvailability};
use super::{
    RecordingCommandEvent, RecordingControlRequest, RecordingDeliverySession, RecordingRunner,
    RecordingStartPolicy, RecordingStartRequest,
};
use crate::audio::{
    AudioEndpoint, AudioEndpointState, AudioInputKind, AudioInputSpec, AudioPlanError,
    AudioSourceMode, MixPlan, MixPlanError, PcmFormat, plan_mix,
};
use crate::protocol::{
    DeliveryRoute, NativeAudioSourceMode, NativeRecordingRegion, NativeRecordingSource,
    NativeRequestedQualitySettings, NativeVideoFrameRate, NativeVideoQuality, OperationAuthority,
    RecordingProgressStatus, VideoMode,
};
use crate::telemetry::{
    KeyCode, KeyModifiers, RecordingTelemetryActionEvent, RecordingTelemetrySignal,
    RecordingTelemetrySource, SanitizedRecordingTelemetrySnapshot, sanitize_key,
};
use crate::transfer::hashing::sha256_hex;

mod encoder;
mod manual_route;
mod platform;
mod state_service;
mod temp_media;

const COMMAND_1_RECORDING_ID: &str =
    "recording-e12c157c0051f399d6ecb3c03a191d6dd199f58abcb4cf1894db2fc30f331ddf";
const COMMAND_1_RECORDING_TIMEBASE_ID: &str =
    "timebase-recording-e12c157c0051f399d6ecb3c03a191d6dd199f58abcb4cf1894db2fc30f331ddf";
const COMMAND_1_RECORDING_JSON: &str =
    "recording-e12c157c0051f399d6ecb3c03a191d6dd199f58abcb4cf1894db2fc30f331ddf.json";
const COMMAND_1_RECORDING_MP4: &str =
    "recording-e12c157c0051f399d6ecb3c03a191d6dd199f58abcb4cf1894db2fc30f331ddf.mp4";

fn encoder_capabilities() -> EncoderCapabilities {
    EncoderCapabilities {
        max_width: 3840,
        max_height: 2160,
        max_fps: 60,
        hardware_available: true,
        software_available: true,
    }
}

fn recording_audio_endpoints() -> Vec<AudioEndpoint> {
    vec![
        AudioEndpoint::microphone(
            "mic-default",
            "Default Microphone",
            true,
            AudioEndpointState::Available,
        ),
        AudioEndpoint::system_loopback(
            "system-default",
            "System Audio",
            AudioEndpointState::Available,
        ),
    ]
}

const fn recording_audio_facts() -> RecordingAudioFacts {
    RecordingAudioFacts {
        target_format: PcmFormat {
            sample_rate_hz: 48_000,
            channels: 2,
        },
        microphone_format: PcmFormat {
            sample_rate_hz: 48_000,
            channels: 1,
        },
        system_loopback_format: PcmFormat {
            sample_rate_hz: 48_000,
            channels: 2,
        },
        microphone_clock_drift_ppm: 0,
        system_loopback_clock_drift_ppm: 15,
    }
}

fn recording_start_command(mode: VideoMode) -> RecordingStartRequest {
    RecordingStartRequest {
        operation_id: "command-1".to_owned(),
        source: NativeRecordingSource {
            mode,
            display_id: None,
            window_id: None,
            region: None,
            microphone_device_id: None,
        },
        authority: OperationAuthority::ExtensionManaged {
            settings_revision: "settings-revision-1".to_owned(),
        },
        delivery_route: DeliveryRoute::ExtensionTransfer,
        settings_revision: "settings-revision-1".to_owned(),
        requested_quality: NativeRequestedQualitySettings {
            audio_bitrate_kbps: 160,
            audio_source_mode: NativeAudioSourceMode::Mixed,
            frame_rate: NativeVideoFrameRate::Auto,
            quality: NativeVideoQuality::High,
            video_bitrate_mbps_override: None,
        },
        include_cursor: true,
        open_editor: true,
        requested_at_epoch_ms: 4_000,
        delivery_session: RecordingDeliverySession {
            controller_lease_id: Some("lease:connection-1".to_owned()),
        },
    }
}

fn recording_region_start_command(region: Option<NativeRecordingRegion>) -> RecordingStartRequest {
    let mut command = recording_start_command(VideoMode::Region);
    command.source.region = region;
    command
}

fn recording_control_command(
    control: crate::protocol::RecordingControl,
    recording_id: &str,
    requested_at_epoch_ms: u64,
) -> RecordingControlRequest {
    RecordingControlRequest {
        operation_id: "command-control-1".to_owned(),
        recording_id: recording_id.to_owned(),
        control,
        requested_at_epoch_ms,
        delivery_session: RecordingDeliverySession {
            controller_lease_id: Some("lease:connection-1".to_owned()),
        },
    }
}

fn recording_transfer_store(
    root: &std::path::Path,
) -> Result<RecordingTransferStore, RecordingRuntimeError> {
    RecordingTransferStore::open(root, RecordingTransferStoreConfig::default())
        .map_err(|_| RecordingRuntimeError::TransferState)
}

fn recording_lifecycle_service<R>(
    root: &std::path::Path,
    runtime: R,
) -> Result<RecordingLifecycleService<R>, RecordingRuntimeError> {
    Ok(RecordingLifecycleService::new_with_test_inputs(
        recording_transfer_store(root)?,
        runtime,
        sufficient_disk_space_probe,
        TestRecordingMonotonicClock::fixed(fixed_monotonic_ns),
    ))
}

fn fixed_monotonic_ns() -> u128 {
    987_654_321
}

fn sufficient_disk_space_probe(
    media_path: &std::path::Path,
) -> Result<RecordingDiskSpace, RecordingDiskGuardError> {
    if !media_path.ends_with(COMMAND_1_RECORDING_MP4) {
        return Err(RecordingDiskGuardError::ProbeFailed);
    }
    Ok(RecordingDiskSpace {
        available_bytes: 20 * 1024 * 1024 * 1024,
        capacity_bytes: 100 * 1024 * 1024 * 1024,
    })
}

fn pressure_disk_space_probe(
    media_path: &std::path::Path,
) -> Result<RecordingDiskSpace, RecordingDiskGuardError> {
    if !media_path.ends_with(COMMAND_1_RECORDING_MP4) {
        return Err(RecordingDiskGuardError::ProbeFailed);
    }
    Ok(RecordingDiskSpace {
        available_bytes: 256 * 1024 * 1024,
        capacity_bytes: 100 * 1024 * 1024 * 1024,
    })
}

#[derive(Clone, Copy, Debug)]
struct FakeRecordingRuntime;

impl RecordingRuntime for FakeRecordingRuntime {
    fn start_recording(
        &mut self,
        request: RecordingRuntimeStartRequest<'_>,
    ) -> Result<StartedRecordingRuntime, RecordingRuntimeError> {
        assert_eq!(request.recording_id, COMMAND_1_RECORDING_ID);
        assert_eq!(request.request.operation_id, "command-1");
        assert!(request.media_path.ends_with(COMMAND_1_RECORDING_MP4));
        Ok(StartedRecordingRuntime {
            effective_quality: effective_quality_from_settings_policy(
                &request.request.requested_quality,
                1920,
                1080,
                recording_quality_limits(),
            )?,
        })
    }

    fn stop_recording(
        &mut self,
        request: RecordingRuntimeStopRequest<'_>,
    ) -> Result<FinalizedRecordingArtifact, RecordingRuntimeError> {
        assert_eq!(request.recording_id, COMMAND_1_RECORDING_ID);
        assert!(request.requested_at_epoch_ms >= 6_000);
        std::fs::write(request.media_path, b"hello world")
            .map_err(|_| RecordingRuntimeError::RuntimeFailed)?;
        Ok(FinalizedRecordingArtifact {
            duration_ms: request.duration_ms,
            width: 1920,
            height: 1080,
            fps: 30,
        })
    }

    fn pause_recording(&mut self, recording_id: &str) -> Result<(), RecordingRuntimeError> {
        assert_eq!(recording_id, COMMAND_1_RECORDING_ID);
        Ok(())
    }

    fn resume_recording(&mut self, recording_id: &str) -> Result<(), RecordingRuntimeError> {
        assert_eq!(recording_id, COMMAND_1_RECORDING_ID);
        Ok(())
    }

    fn cancel_recording(&mut self, _recording_id: &str) -> Result<(), RecordingRuntimeError> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
struct FixedTelemetrySource;

impl RecordingTelemetrySource for FixedTelemetrySource {
    fn snapshot_for_recording(
        &mut self,
        recording_id: &str,
        duration_ms: u64,
    ) -> Option<SanitizedRecordingTelemetrySnapshot> {
        assert_eq!(recording_id, COMMAND_1_RECORDING_ID);
        assert_eq!(duration_ms, 2_000);
        let action = RecordingTelemetryActionEvent::from_key_signal(
            sanitize_key(KeyCode::Character('s'), KeyModifiers::CTRL),
            1,
        )?;
        Some(SanitizedRecordingTelemetrySnapshot::new(
            vec![action],
            vec![RecordingTelemetrySignal::typing_span(3, 1, 2)],
        ))
    }
}

#[derive(Clone, Debug, Default)]
struct SharedLifecycleTelemetry {
    events: Rc<RefCell<Vec<TelemetryLifecycleEvent>>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TelemetryLifecycleEvent {
    Start(String, u64),
    Pause(String, u64),
    Resume(String, u64),
    Finish(String, u64),
    Cancel(String, u64),
    Discard(String),
    Snapshot(String, u64),
}

impl SharedLifecycleTelemetry {
    fn events(&self) -> Vec<TelemetryLifecycleEvent> {
        self.events.borrow().clone()
    }

    fn push(&mut self, event: TelemetryLifecycleEvent) {
        self.events.borrow_mut().push(event);
    }
}

impl RecordingTelemetrySource for SharedLifecycleTelemetry {
    fn start_recording(&mut self, recording_id: &str, started_at_monotonic_ns: u64) {
        self.push(TelemetryLifecycleEvent::Start(
            recording_id.to_owned(),
            started_at_monotonic_ns,
        ));
    }

    fn pause_recording(&mut self, recording_id: &str, paused_at_monotonic_ns: u64) {
        self.push(TelemetryLifecycleEvent::Pause(
            recording_id.to_owned(),
            paused_at_monotonic_ns,
        ));
    }

    fn resume_recording(&mut self, recording_id: &str, resumed_at_monotonic_ns: u64) {
        self.push(TelemetryLifecycleEvent::Resume(
            recording_id.to_owned(),
            resumed_at_monotonic_ns,
        ));
    }

    fn finish_recording(&mut self, recording_id: &str, stopped_at_monotonic_ns: u64) {
        self.push(TelemetryLifecycleEvent::Finish(
            recording_id.to_owned(),
            stopped_at_monotonic_ns,
        ));
    }

    fn cancel_recording(&mut self, recording_id: &str, cancelled_at_monotonic_ns: u64) {
        self.push(TelemetryLifecycleEvent::Cancel(
            recording_id.to_owned(),
            cancelled_at_monotonic_ns,
        ));
    }

    fn discard_recording(&mut self, recording_id: &str) {
        self.push(TelemetryLifecycleEvent::Discard(recording_id.to_owned()));
    }

    fn snapshot_for_recording(
        &mut self,
        recording_id: &str,
        duration_ms: u64,
    ) -> Option<SanitizedRecordingTelemetrySnapshot> {
        self.push(TelemetryLifecycleEvent::Snapshot(
            recording_id.to_owned(),
            duration_ms,
        ));
        None
    }
}

#[derive(Clone, Copy, Debug)]
struct FailingStartRuntime;

impl RecordingRuntime for FailingStartRuntime {
    fn start_recording(
        &mut self,
        _request: RecordingRuntimeStartRequest<'_>,
    ) -> Result<StartedRecordingRuntime, RecordingRuntimeError> {
        Err(RecordingRuntimeError::RuntimeFailed)
    }

    fn stop_recording(
        &mut self,
        _request: RecordingRuntimeStopRequest<'_>,
    ) -> Result<FinalizedRecordingArtifact, RecordingRuntimeError> {
        Err(RecordingRuntimeError::InvalidState)
    }

    fn pause_recording(&mut self, _recording_id: &str) -> Result<(), RecordingRuntimeError> {
        Err(RecordingRuntimeError::InvalidState)
    }

    fn resume_recording(&mut self, _recording_id: &str) -> Result<(), RecordingRuntimeError> {
        Err(RecordingRuntimeError::InvalidState)
    }

    fn cancel_recording(&mut self, _recording_id: &str) -> Result<(), RecordingRuntimeError> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
struct FailingStopRuntime;

impl RecordingRuntime for FailingStopRuntime {
    fn start_recording(
        &mut self,
        _request: RecordingRuntimeStartRequest<'_>,
    ) -> Result<StartedRecordingRuntime, RecordingRuntimeError> {
        Ok(StartedRecordingRuntime {
            effective_quality: effective_quality_from_settings_policy(
                &recording_start_command(VideoMode::Screen).requested_quality,
                1920,
                1080,
                recording_quality_limits(),
            )?,
        })
    }

    fn stop_recording(
        &mut self,
        request: RecordingRuntimeStopRequest<'_>,
    ) -> Result<FinalizedRecordingArtifact, RecordingRuntimeError> {
        std::fs::write(request.media_path, b"partial")
            .map_err(|_| RecordingRuntimeError::RuntimeFailed)?;
        Err(RecordingRuntimeError::RuntimeFailed)
    }

    fn pause_recording(&mut self, recording_id: &str) -> Result<(), RecordingRuntimeError> {
        let mut runtime = FakeRecordingRuntime;
        runtime.pause_recording(recording_id)
    }

    fn resume_recording(&mut self, recording_id: &str) -> Result<(), RecordingRuntimeError> {
        let mut runtime = FakeRecordingRuntime;
        runtime.resume_recording(recording_id)
    }

    fn cancel_recording(&mut self, _recording_id: &str) -> Result<(), RecordingRuntimeError> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
struct FailingPauseRuntime;

impl RecordingRuntime for FailingPauseRuntime {
    fn start_recording(
        &mut self,
        request: RecordingRuntimeStartRequest<'_>,
    ) -> Result<StartedRecordingRuntime, RecordingRuntimeError> {
        let mut runtime = FakeRecordingRuntime;
        runtime.start_recording(request)
    }

    fn stop_recording(
        &mut self,
        request: RecordingRuntimeStopRequest<'_>,
    ) -> Result<FinalizedRecordingArtifact, RecordingRuntimeError> {
        let mut runtime = FakeRecordingRuntime;
        runtime.stop_recording(request)
    }

    fn pause_recording(&mut self, _recording_id: &str) -> Result<(), RecordingRuntimeError> {
        Err(RecordingRuntimeError::RuntimeFailed)
    }

    fn resume_recording(&mut self, recording_id: &str) -> Result<(), RecordingRuntimeError> {
        let mut runtime = FakeRecordingRuntime;
        runtime.resume_recording(recording_id)
    }

    fn cancel_recording(&mut self, _recording_id: &str) -> Result<(), RecordingRuntimeError> {
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
struct InvalidArtifactRuntime;

impl RecordingRuntime for InvalidArtifactRuntime {
    fn start_recording(
        &mut self,
        request: RecordingRuntimeStartRequest<'_>,
    ) -> Result<StartedRecordingRuntime, RecordingRuntimeError> {
        let mut runtime = FakeRecordingRuntime;
        runtime.start_recording(request)
    }

    fn stop_recording(
        &mut self,
        request: RecordingRuntimeStopRequest<'_>,
    ) -> Result<FinalizedRecordingArtifact, RecordingRuntimeError> {
        std::fs::write(request.media_path, b"hello world")
            .map_err(|_| RecordingRuntimeError::RuntimeFailed)?;
        Ok(FinalizedRecordingArtifact {
            duration_ms: 2_000,
            width: 0,
            height: 1080,
            fps: 30,
        })
    }

    fn pause_recording(&mut self, recording_id: &str) -> Result<(), RecordingRuntimeError> {
        let mut runtime = FakeRecordingRuntime;
        runtime.pause_recording(recording_id)
    }

    fn resume_recording(&mut self, recording_id: &str) -> Result<(), RecordingRuntimeError> {
        let mut runtime = FakeRecordingRuntime;
        runtime.resume_recording(recording_id)
    }

    fn cancel_recording(&mut self, _recording_id: &str) -> Result<(), RecordingRuntimeError> {
        Ok(())
    }
}

fn recording_quality_limits() -> RecordingQualityLimits {
    RecordingQualityLimits {
        max_fps: 60,
        max_width: 3840,
        max_height: 2160,
        hardware_encoder_available: false,
    }
}

fn encoder_request() -> EncoderRequest {
    EncoderRequest {
        width: 1920,
        height: 1080,
        fps: 30,
        video_bitrate_mbps: 12.0,
        audio_bitrate_kbps: 160,
        audio_mode: EncoderAudioMode::Mixed,
        prefer_hardware: true,
    }
}

fn mixed_audio_plan() -> Result<MixPlan, MixPlanError> {
    plan_mix(
        AudioSourceMode::Mixed,
        Some(AudioInputSpec {
            kind: AudioInputKind::Microphone,
            format: PcmFormat {
                sample_rate_hz: 48_000,
                channels: 2,
            },
            clock_drift_ppm: 0,
        }),
        Some(AudioInputSpec {
            kind: AudioInputKind::SystemLoopback,
            format: PcmFormat {
                sample_rate_hz: 48_000,
                channels: 2,
            },
            clock_drift_ppm: 0,
        }),
        PcmFormat {
            sample_rate_hz: 48_000,
            channels: 2,
        },
    )
}

fn read_test_file(path: &std::path::Path) -> Result<Vec<u8>, std::io::Error> {
    let mut file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn write_legacy_finalized_recording_metadata(
    root: &std::path::Path,
    recording_id: &str,
) -> Result<(), std::io::Error> {
    let payload = format!(
        r#"{{
            "media_id":"{recording_id}",
            "state":"Finalized",
            "created_at_ms":1000,
            "updated_at_ms":2000,
            "total_bytes":11,
            "sha256":"{}",
            "retain_rejected_until_ms":null,
            "recording":{{
                "filename":"{recording_id}.mp4",
                "duration_ms":5000,
                "width":1920,
                "height":1080,
                "fps":30,
                "open_editor":true
            }}
        }}"#,
        sha256_hex(b"hello world")
    );
    std::fs::write(root.join(format!("{recording_id}.json")), payload)
}

fn tamper_last_byte(path: &std::path::Path) -> Result<(), std::io::Error> {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    file.seek(SeekFrom::End(-1))?;
    let mut byte = [0_u8; 1];
    file.read_exact(&mut byte)?;
    file.seek(SeekFrom::End(-1))?;
    file.write_all(&[byte[0] ^ 0xff])
}
