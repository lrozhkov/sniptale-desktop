//! Joint retention waits for the independently durable video, cursor, and event receipts.

use super::agent::AgentSideEffects;
use crate::{
    app::responses::NativeHostSideEffectError,
    protocol::trajectory::CursorTrajectoryState,
    transfer::{
        browser_delivery::{BrowserMediaKind, RecordingHistoryBinding},
        trajectory_store::TrajectoryVideoBinding,
    },
};

impl AgentSideEffects {
    pub(super) fn accept_video_attachment_receipt(
        &mut self,
        recording: &str,
    ) -> Result<(), NativeHostSideEffectError> {
        let pending = self
            .pending_recordings_for_reconnect_from_store()?
            .into_iter()
            .find(|item| item.recording_id == recording);
        let Some(pending) = pending else {
            return Ok(());
        };
        let Some(target) = self.browser_target_id.as_deref() else {
            return if pending.cursor_trajectory.is_none() && pending.recording_events.is_none() {
                Ok(())
            } else {
                Err(NativeHostSideEffectError::RecordingTransferState)
            };
        };
        let store = self
            .recordings
            .as_ref()
            .ok_or(NativeHostSideEffectError::RecordingTransferState)?;
        let cursors = store.cursor_trajectory_catalog();
        let events = store.recording_events_catalog();
        let mut histories = RecordingHistoryBinding {
            cursor_trajectory_id: None,
            event_stream_id: None,
        };
        if let Some(descriptor) = pending.cursor_trajectory {
            let present = cursors
                .lookup(recording, &descriptor.trajectory_id)
                .map_err(|_| NativeHostSideEffectError::RecordingTransferState)?
                .is_some();
            if present {
                histories.cursor_trajectory_id = Some(descriptor.trajectory_id);
            } else if descriptor.state != CursorTrajectoryState::Unavailable
                || descriptor.total_blocks != 0
            {
                return Err(NativeHostSideEffectError::RecordingTransferState);
            }
        }
        if let Some(descriptor) = pending.recording_events {
            let present = events
                .lookup(recording, &descriptor.event_stream_id)
                .map_err(|_| NativeHostSideEffectError::RecordingTransferState)?
                .is_some();
            if present {
                histories.event_stream_id = Some(descriptor.event_stream_id);
            } else if descriptor.state != CursorTrajectoryState::Unavailable
                || descriptor.total_blocks != 0
            {
                return Err(NativeHostSideEffectError::RecordingTransferState);
            }
        }
        let assignments = self.assignment_store()?;
        // Freeze the complete attachment set before either receipt or video cleanup can advance.
        assignments
            .bind_recording_histories(recording, target, histories.clone())
            .map_err(|_| NativeHostSideEffectError::RecordingTransferState)?;
        let video = TrajectoryVideoBinding {
            sha256: pending.sha256,
            bytes: pending.total_bytes,
            duration_ms: pending.duration_ms,
            width: pending.width,
            height: pending.height,
        };
        if let Some(source) = histories.cursor_trajectory_id {
            let mut archive = cursors
                .authorized_mutation(recording, &source, target, &assignments)
                .map_err(|_| NativeHostSideEffectError::RecordingTransferState)?;
            archive
                .bind_video(video.clone())
                .and_then(|()| archive.accept_video_receipt(video.bytes, &video.sha256))
                .map_err(|_| NativeHostSideEffectError::RecordingTransferState)?;
        }
        if let Some(source) = histories.event_stream_id {
            let mut archive = events
                .authorized_mutation(recording, &source, target, &assignments)
                .map_err(|_| NativeHostSideEffectError::RecordingTransferState)?;
            archive
                .bind_video(video.clone())
                .and_then(|()| archive.accept_video_receipt(video.bytes, &video.sha256))
                .map_err(|_| NativeHostSideEffectError::RecordingTransferState)?;
        }
        Ok(())
    }

    pub(super) fn cleanup_recording_history_assignment(
        &self,
        recording: &str,
    ) -> Result<bool, NativeHostSideEffectError> {
        let target = self
            .browser_target_id
            .as_deref()
            .ok_or(NativeHostSideEffectError::RecordingTransferState)?;
        let assignments = self.assignment_store()?;
        let Some(histories) = assignments
            .recording_histories(recording, target)
            .map_err(|_| NativeHostSideEffectError::RecordingTransferState)?
        else {
            return Ok(false);
        };
        let store = self
            .recordings
            .as_ref()
            .ok_or(NativeHostSideEffectError::RecordingTransferState)?;
        let cursors = store.cursor_trajectory_catalog();
        let events = store.recording_events_catalog();
        if let Some(source) = &histories.cursor_trajectory_id {
            let item = cursors
                .authorized(recording, source, target, &assignments)
                .map_err(|_| NativeHostSideEffectError::RecordingTransferState)?;
            if !item.video_receipt_accepted || !item.attachment_receipt_accepted {
                return Ok(false);
            }
        }
        if let Some(source) = &histories.event_stream_id {
            let item = events
                .authorized(recording, source, target, &assignments)
                .map_err(|_| NativeHostSideEffectError::RecordingTransferState)?;
            if !item.video_receipt_accepted || !item.event_receipt_accepted {
                return Ok(false);
            }
        }
        assignments
            .remove_completed(recording, BrowserMediaKind::Recording)
            .map_err(|_| NativeHostSideEffectError::RecordingTransferState)?;
        // All sole copies have durable browser receipts. Auxiliary cleanup may retain extra
        // protected data, but cannot revoke those receipts or authorize another browser.
        if let Some(source) = histories.cursor_trajectory_id {
            let _ = cursors.erase_acknowledged(recording, &source);
        }
        if let Some(source) = histories.event_stream_id {
            let _ = events.erase_acknowledged(recording, &source);
        }
        Ok(true)
    }
}
