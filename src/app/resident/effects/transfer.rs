//! Transfer-store read, ack, reject, and reconnect side effects.

use super::agent::AgentSideEffects;
use crate::{
    app::responses::{
        NativeHostSideEffectError, NativeHostTransferChunk, NativeHostTransferChunkError,
    },
    capture::{PendingScreenshotTransfer, ScreenshotTransferStore, ScreenshotTransferStoreError},
    recording::{PendingRecordingTransfer, RecordingTransferStore, RecordingTransferStoreError},
    transfer::{chunks::EncodedChunk, error::TransferError},
};

impl AgentSideEffects {
    pub(super) fn pending_recordings_for_reconnect_from_store(
        &mut self,
    ) -> Result<Vec<PendingRecordingTransfer>, NativeHostSideEffectError> {
        self.recordings()?
            .pending_recordings_for_reconnect()
            .map_err(|_| NativeHostSideEffectError::RecordingRecovery)
    }

    pub(super) fn pending_screenshots_for_reconnect_from_store(
        &mut self,
    ) -> Result<Vec<PendingScreenshotTransfer>, NativeHostSideEffectError> {
        self.screenshots()?
            .pending_screenshots_for_reconnect()
            .map_err(|_| NativeHostSideEffectError::ScreenshotTransferState)
    }

    pub(super) fn ack_screenshot_transfer_in_store(
        &mut self,
        capture_id: &str,
    ) -> Result<bool, NativeHostSideEffectError> {
        self.screenshots()?
            .ack_screenshot(capture_id)
            .map_err(|_| NativeHostSideEffectError::ScreenshotTransferState)
    }

    pub(super) fn reject_screenshot_transfer_in_store(
        &mut self,
        capture_id: &str,
        retain_for_diagnostics: bool,
    ) -> Result<bool, NativeHostSideEffectError> {
        self.screenshots()?
            .reject_screenshot(capture_id, retain_for_diagnostics)
            .map_err(|_| NativeHostSideEffectError::ScreenshotTransferState)
    }

    pub(super) fn screenshot_transfer_chunk_from_store(
        &mut self,
        capture_id: &str,
        chunk_index: u32,
        now_ms: u64,
    ) -> Result<Option<NativeHostTransferChunk>, NativeHostTransferChunkError> {
        self.screenshots()
            .map_err(|_| NativeHostTransferChunkError::State)?
            .screenshot_chunk_at(capture_id, chunk_index, now_ms)
            .map(|chunk| {
                chunk.map(|chunk| NativeHostTransferChunk {
                    chunk: chunk.chunk,
                    is_complete: chunk.is_complete,
                })
            })
            .map_err(|error| screenshot_chunk_error(&error))
    }

    pub(super) fn ack_recording_transfer_in_store(
        &mut self,
        recording_id: &str,
    ) -> Result<bool, NativeHostSideEffectError> {
        self.recordings()?
            .ack_recording(recording_id)
            .map_err(|_| NativeHostSideEffectError::RecordingTransferState)
    }

    pub(super) fn reject_recording_transfer_in_store(
        &mut self,
        recording_id: &str,
        retain_for_diagnostics: bool,
    ) -> Result<bool, NativeHostSideEffectError> {
        self.recordings()?
            .reject_recording(recording_id, retain_for_diagnostics)
            .map_err(|_| NativeHostSideEffectError::RecordingTransferState)
    }

    pub(super) fn recording_transfer_chunk_from_store(
        &mut self,
        recording_id: &str,
        chunk_index: u32,
        now_ms: u64,
    ) -> Result<Option<EncodedChunk>, NativeHostTransferChunkError> {
        self.recordings()
            .map_err(|_| NativeHostTransferChunkError::State)?
            .recording_chunk_at(recording_id, chunk_index, now_ms)
            .map_err(|error| recording_chunk_error(&error))
    }

    fn screenshots(&self) -> Result<&ScreenshotTransferStore, NativeHostSideEffectError> {
        self.screenshots
            .as_ref()
            .ok_or(NativeHostSideEffectError::ScreenshotTransferState)
    }

    fn recordings(&self) -> Result<&RecordingTransferStore, NativeHostSideEffectError> {
        self.recordings
            .as_ref()
            .ok_or(NativeHostSideEffectError::RecordingTransferState)
    }
}

fn screenshot_chunk_error(error: &ScreenshotTransferStoreError) -> NativeHostTransferChunkError {
    match error {
        ScreenshotTransferStoreError::Transfer(TransferError::ChunkIndexOutOfBounds { .. }) => {
            NativeHostTransferChunkError::ChunkOutOfBounds
        }
        ScreenshotTransferStoreError::Transfer(TransferError::OutOfOrderChunk { .. }) => {
            NativeHostTransferChunkError::Replay
        }
        ScreenshotTransferStoreError::Transfer(TransferError::ProtectedMedia) => {
            NativeHostTransferChunkError::ProtectedMedia
        }
        _ => NativeHostTransferChunkError::State,
    }
}

fn recording_chunk_error(error: &RecordingTransferStoreError) -> NativeHostTransferChunkError {
    match error {
        RecordingTransferStoreError::Transfer(TransferError::ChunkIndexOutOfBounds { .. }) => {
            NativeHostTransferChunkError::ChunkOutOfBounds
        }
        RecordingTransferStoreError::Transfer(TransferError::OutOfOrderChunk { .. }) => {
            NativeHostTransferChunkError::Replay
        }
        RecordingTransferStoreError::Transfer(TransferError::ProtectedMedia) => {
            NativeHostTransferChunkError::ProtectedMedia
        }
        _ => NativeHostTransferChunkError::State,
    }
}
