//! Bounded WASAPI packet accounting for recording diagnostics.

use super::{
    super::mixer::AudioInputKind,
    format::{data_discontinuity_flag, silent_flag, timestamp_error_flag},
};

#[derive(Debug, Default)]
pub(super) struct CaptureMetrics {
    packets: u64,
    frames: u64,
    pending_polls: u64,
    silent_packets: u64,
    discontinuities: u64,
    timestamp_errors: u64,
    gap_frames: u64,
}

impl CaptureMetrics {
    pub(super) fn observe_gap(&mut self, frames: u64) {
        self.gap_frames = self.gap_frames.saturating_add(frames);
    }
    pub(super) fn observe_pending(&mut self) {
        self.pending_polls = self.pending_polls.saturating_add(1);
    }

    pub(super) fn observe_packet(&mut self, frames: u32, flags: u32) {
        self.packets = self.packets.saturating_add(1);
        self.frames = self.frames.saturating_add(u64::from(frames));
        if flags & silent_flag() != 0 {
            self.silent_packets = self.silent_packets.saturating_add(1);
        }
        if flags & data_discontinuity_flag() != 0 {
            self.discontinuities = self.discontinuities.saturating_add(1);
        }
        if flags & timestamp_error_flag() != 0 {
            self.timestamp_errors = self.timestamp_errors.saturating_add(1);
        }
    }

    pub(super) fn log(&self, kind: AudioInputKind, buffer_frames: u32) {
        tracing::info!(
            target: "sniptale_native_agent::recording",
            phase = "audio-source-summary",
            ?kind,
            buffer_frames,
            packet_count = self.packets,
            frame_count = self.frames,
            pending_poll_count = self.pending_polls,
            silent_packet_count = self.silent_packets,
            discontinuity_count = self.discontinuities,
            timestamp_error_count = self.timestamp_errors,
            gap_frame_count = self.gap_frames,
            "recording audio source stopped"
        );
    }
}
