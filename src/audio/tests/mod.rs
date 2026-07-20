mod adapters;
mod devices;
mod mix_plan;
mod pcm_mixer;
mod pipeline;

use std::collections::VecDeque;

use super::{
    devices::{AudioEndpoint, AudioEndpointState},
    mixer::{AudioInputKind, PcmFormat},
    pcm_source::{OwnedPcmInputChunk, PcmChunkSource, PcmSourceError, PcmSourcePoll},
};

fn target_format() -> PcmFormat {
    PcmFormat {
        sample_rate_hz: 48_000,
        channels: 2,
    }
}

fn mono_format() -> PcmFormat {
    PcmFormat {
        sample_rate_hz: 48_000,
        channels: 1,
    }
}

fn available_microphone(id: &str, is_default: bool) -> AudioEndpoint {
    AudioEndpoint::microphone(id, "Microphone", is_default, AudioEndpointState::Available)
}

fn available_system() -> AudioEndpoint {
    AudioEndpoint::system_loopback(
        "system-default",
        "System Audio",
        AudioEndpointState::Available,
    )
}

fn input_spec(
    kind: AudioInputKind,
    format: PcmFormat,
    clock_drift_ppm: i32,
) -> super::AudioInputSpec {
    super::AudioInputSpec {
        kind,
        format,
        clock_drift_ppm,
    }
}

fn owned_chunk(
    kind: AudioInputKind,
    format: PcmFormat,
    samples: &[i16],
    timestamp_100ns: i64,
    duration_100ns: i64,
) -> OwnedPcmInputChunk {
    OwnedPcmInputChunk {
        kind,
        format,
        bytes: pcm_bytes(samples),
        timestamp_100ns,
        duration_100ns,
    }
}

fn pcm_bytes(samples: &[i16]) -> Vec<u8> {
    samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect()
}

fn decode_i16(bytes: &[u8]) -> Vec<i16> {
    bytes
        .chunks_exact(2)
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
        .collect()
}

#[derive(Debug)]
struct FakePcmSource {
    chunks: VecDeque<Result<PcmSourcePoll, PcmSourceError>>,
    polls: usize,
}

impl FakePcmSource {
    fn new(chunks: Vec<Result<PcmSourcePoll, PcmSourceError>>) -> Self {
        Self {
            chunks: VecDeque::from(chunks),
            polls: 0,
        }
    }
}

impl PcmChunkSource for FakePcmSource {
    fn next_pcm_chunk(&mut self) -> Result<PcmSourcePoll, PcmSourceError> {
        self.polls += 1;
        self.chunks
            .pop_front()
            .unwrap_or(Ok(PcmSourcePoll::Drained))
    }
}
