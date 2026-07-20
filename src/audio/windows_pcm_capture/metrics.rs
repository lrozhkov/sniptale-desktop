//! Bounded WASAPI packet accounting for recording diagnostics.

use super::{
    super::mixer::AudioInputKind,
    format::{data_discontinuity_flag, silent_flag, timestamp_error_flag},
};

#[derive(Debug, Default)]
pub(super) struct CaptureMetrics {
    packet_count: u64,
    frame_count: u64,
    pending_poll_count: u64,
    silent_packet_count: u64,
    discontinuity_count: u64,
    timestamp_error_count: u64,
}

impl CaptureMetrics {
    pub(super) fn observe_pending(&mut self) {
        self.pending_poll_count = self.pending_poll_count.saturating_add(1);
    }

    pub(super) fn observe_packet(&mut self, frames: u32, flags: u32) {
        self.packet_count = self.packet_count.saturating_add(1);
        self.frame_count = self.frame_count.saturating_add(u64::from(frames));
        if flags & silent_flag() != 0 {
            self.silent_packet_count = self.silent_packet_count.saturating_add(1);
        }
        if flags & data_discontinuity_flag() != 0 {
            self.discontinuity_count = self.discontinuity_count.saturating_add(1);
        }
        if flags & timestamp_error_flag() != 0 {
            self.timestamp_error_count = self.timestamp_error_count.saturating_add(1);
        }
    }

    pub(super) fn log(&self, kind: AudioInputKind, buffer_frames: u32) {
        tracing::info!(
            target: "sniptale_native_agent::recording",
            phase = "audio-source-summary",
            ?kind,
            buffer_frames,
            packet_count = self.packet_count,
            frame_count = self.frame_count,
            pending_poll_count = self.pending_poll_count,
            silent_packet_count = self.silent_packet_count,
            discontinuity_count = self.discontinuity_count,
            timestamp_error_count = self.timestamp_error_count,
            "recording audio source stopped"
        );
    }
}
