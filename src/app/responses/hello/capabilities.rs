//! Native-host capability payload mapping.

use crate::{
    audio::{AudioCapabilitySummary, AudioUnavailableReason, runtime_audio_capability_summary},
    capture::{
        RegionOverlayAvailability, WgcRuntimeAvailability, runtime_capture_availability,
        runtime_region_overlay_availability,
    },
    protocol::{
        AudioCodec, CHUNK_RAW_BYTES, ContainerCodec, NativeAppCapabilities,
        NativeAudioCapabilities, NativeCapabilityLimits, NativeCapabilityUnavailableReason,
        NativeCaptureCapabilities, NativeCodecCapabilities, RECORDING_MAX_TOTAL_BYTES,
        SCREENSHOT_MAX_TOTAL_BYTES, ScreenshotMode, VideoCodec, VideoMode,
    },
    recording::{
        MediaFoundationAvailability, RecordingRuntimeAvailability, VideoEncoderAvailability,
        media_foundation_availability, recording_runtime_availability, video_encoder_availability,
    },
};

pub(in crate::app) struct CapabilityFacts {
    pub(in crate::app) capture: WgcRuntimeAvailability,
    pub(in crate::app) region_overlay: RegionOverlayAvailability,
    pub(in crate::app) recording: RecordingRuntimeAvailability,
    pub(in crate::app) encoder: MediaFoundationAvailability,
    pub(in crate::app) video_encoder: VideoEncoderAvailability,
    pub(in crate::app) audio: AudioCapabilitySummary,
}

pub(in crate::app) fn runtime_capability_snapshot() -> NativeAppCapabilities {
    capabilities_from_facts(runtime_capability_facts())
}

pub(super) fn handshake_capability_snapshot() -> NativeAppCapabilities {
    capabilities_from_facts(handshake_capability_facts())
}

pub(in crate::app) fn capabilities_from_facts(facts: CapabilityFacts) -> NativeAppCapabilities {
    let audio_unavailable_reasons = audio_unavailable_reasons(&facts.audio);
    NativeAppCapabilities {
        capture: NativeCaptureCapabilities {
            screenshot_modes: screenshot_modes(facts.capture, facts.region_overlay),
            video_modes: video_modes(facts.capture, facts.region_overlay, facts.recording),
            supports_freeze_region_selection: supports_freeze_region_selection(
                facts.region_overlay,
            ),
        },
        codecs: NativeCodecCapabilities {
            containers: container_codecs(facts.encoder, facts.video_encoder),
            video: video_codecs(facts.encoder, facts.video_encoder),
            audio: audio_codecs(facts.encoder),
            hardware_encoder_available: facts.video_encoder.h264_hardware_available,
            unavailable_reasons: codec_unavailable_reasons(facts.encoder, facts.video_encoder),
        },
        audio: NativeAudioCapabilities {
            microphone_devices: facts
                .audio
                .microphone_devices
                .into_iter()
                .map(|device| crate::protocol::NativeMicrophoneDevice {
                    id: device.id,
                    label: device.label,
                    is_default: device.is_default,
                    available: device.available,
                })
                .collect(),
            supports_microphone: facts.audio.supports_microphone,
            supports_system_audio: facts.audio.supports_system_audio,
            supports_mixed_audio: facts.audio.supports_mixed_audio,
            unavailable_reasons: audio_unavailable_reasons,
        },
        limits: NativeCapabilityLimits {
            max_chunk_bytes: CHUNK_RAW_BYTES,
            max_recording_bytes: RECORDING_MAX_TOTAL_BYTES,
            max_screenshot_bytes: SCREENSHOT_MAX_TOTAL_BYTES,
            max_fps: 60,
            max_width: 7680,
            max_height: 4320,
        },
    }
}

fn runtime_capability_facts() -> CapabilityFacts {
    CapabilityFacts {
        capture: runtime_capture_availability(),
        region_overlay: runtime_region_overlay_availability(),
        recording: recording_runtime_availability(),
        encoder: media_foundation_availability(),
        video_encoder: video_encoder_availability(),
        audio: runtime_audio_capability_summary(),
    }
}

fn handshake_capability_facts() -> CapabilityFacts {
    let capture = runtime_capture_availability();
    CapabilityFacts {
        capture,
        region_overlay: runtime_region_overlay_availability(),
        recording: handshake_recording_availability(capture),
        encoder: handshake_encoder_availability(),
        video_encoder: handshake_video_encoder_availability(),
        audio: handshake_audio_summary(),
    }
}

const fn handshake_recording_availability(
    capture: WgcRuntimeAvailability,
) -> RecordingRuntimeAvailability {
    match capture {
        WgcRuntimeAvailability::Available => RecordingRuntimeAvailability::Available,
        WgcRuntimeAvailability::UnsupportedPlatform => {
            RecordingRuntimeAvailability::UnsupportedPlatform
        }
        WgcRuntimeAvailability::ApiUnavailable => RecordingRuntimeAvailability::RuntimeUnavailable,
    }
}

const fn handshake_encoder_availability() -> MediaFoundationAvailability {
    if cfg!(windows) {
        MediaFoundationAvailability::Available
    } else {
        MediaFoundationAvailability::UnsupportedPlatform
    }
}

const fn handshake_video_encoder_availability() -> VideoEncoderAvailability {
    VideoEncoderAvailability {
        h264_available: cfg!(windows),
        h264_hardware_available: false,
    }
}

fn handshake_audio_summary() -> AudioCapabilitySummary {
    AudioCapabilitySummary {
        microphone_devices: Vec::new(),
        supports_microphone: false,
        supports_system_audio: false,
        supports_mixed_audio: false,
        unavailable_reasons: vec![AudioUnavailableReason::UnsupportedCapability],
    }
}

fn supports_freeze_region_selection(availability: RegionOverlayAvailability) -> bool {
    availability == RegionOverlayAvailability::Available
}

fn supports_region_selection(availability: RegionOverlayAvailability) -> bool {
    matches!(
        availability,
        RegionOverlayAvailability::SelectionAvailable | RegionOverlayAvailability::Available
    )
}

fn screenshot_modes(
    capture: WgcRuntimeAvailability,
    overlay: RegionOverlayAvailability,
) -> Vec<ScreenshotMode> {
    if capture != WgcRuntimeAvailability::Available {
        return Vec::new();
    }
    let mut modes = vec![
        ScreenshotMode::Screen,
        ScreenshotMode::ActiveWindow,
        ScreenshotMode::AllScreens,
    ];
    if supports_region_selection(overlay) {
        modes.push(ScreenshotMode::Region);
    }
    modes
}

fn video_modes(
    capture: WgcRuntimeAvailability,
    overlay: RegionOverlayAvailability,
    recording: RecordingRuntimeAvailability,
) -> Vec<VideoMode> {
    if capture != WgcRuntimeAvailability::Available
        || recording != RecordingRuntimeAvailability::Available
    {
        return Vec::new();
    }
    let mut modes = vec![VideoMode::Screen, VideoMode::ActiveWindow];
    if supports_region_selection(overlay) {
        modes.push(VideoMode::Region);
    }
    modes
}

fn container_codecs(
    availability: MediaFoundationAvailability,
    encoder: VideoEncoderAvailability,
) -> Vec<ContainerCodec> {
    if availability == MediaFoundationAvailability::Available && encoder.h264_available {
        vec![ContainerCodec::Mp4]
    } else {
        Vec::new()
    }
}

fn video_codecs(
    availability: MediaFoundationAvailability,
    encoder: VideoEncoderAvailability,
) -> Vec<VideoCodec> {
    if availability == MediaFoundationAvailability::Available && encoder.h264_available {
        vec![VideoCodec::H264]
    } else {
        Vec::new()
    }
}

fn audio_codecs(availability: MediaFoundationAvailability) -> Vec<AudioCodec> {
    if availability == MediaFoundationAvailability::Available {
        vec![AudioCodec::Aac]
    } else {
        Vec::new()
    }
}

fn codec_unavailable_reasons(
    availability: MediaFoundationAvailability,
    encoder: VideoEncoderAvailability,
) -> Vec<NativeCapabilityUnavailableReason> {
    match availability {
        MediaFoundationAvailability::Available if encoder.h264_available => Vec::new(),
        MediaFoundationAvailability::UnsupportedPlatform => {
            vec![NativeCapabilityUnavailableReason::UnsupportedPlatform]
        }
        MediaFoundationAvailability::Available
        | MediaFoundationAvailability::RuntimeUnavailable => {
            vec![NativeCapabilityUnavailableReason::EncoderMissing]
        }
    }
}

fn audio_unavailable_reasons(
    summary: &AudioCapabilitySummary,
) -> Vec<NativeCapabilityUnavailableReason> {
    let mut mapped = Vec::new();
    for reason in &summary.unavailable_reasons {
        push_unique_reason(&mut mapped, audio_unavailable_reason(*reason));
    }
    if mapped.is_empty()
        && !(summary.supports_microphone
            && summary.supports_system_audio
            && summary.supports_mixed_audio)
    {
        mapped.push(NativeCapabilityUnavailableReason::Unknown);
    }
    mapped
}

const fn audio_unavailable_reason(
    reason: AudioUnavailableReason,
) -> NativeCapabilityUnavailableReason {
    match reason {
        #[cfg(test)]
        AudioUnavailableReason::PermissionDenied => {
            NativeCapabilityUnavailableReason::PermissionDenied
        }
        #[cfg(test)]
        AudioUnavailableReason::PolicyDenied => NativeCapabilityUnavailableReason::PolicyDenied,
        #[cfg(test)]
        AudioUnavailableReason::DeviceBusy => NativeCapabilityUnavailableReason::DeviceBusy,
        #[cfg(test)]
        AudioUnavailableReason::AudioSourceLost => NativeCapabilityUnavailableReason::Unknown,
        AudioUnavailableReason::UnsupportedCapability => NativeCapabilityUnavailableReason::Unknown,
    }
}

fn push_unique_reason(
    reasons: &mut Vec<NativeCapabilityUnavailableReason>,
    reason: NativeCapabilityUnavailableReason,
) {
    if !reasons.contains(&reason) {
        reasons.push(reason);
    }
}
