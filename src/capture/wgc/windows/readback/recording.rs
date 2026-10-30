//! Persistent recording frame polling and typed WGC diagnostics.

use super::{
    CaptureSession, D3dDevice, ReadbackFrame, is_empty_frame_poll_hresult, readback_texture,
    texture_from_surface, wait_for_recording_frame_retry,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::capture::wgc::windows) enum RecordingFrameReadError {
    Poll(i32),
    Timestamp(i32),
    Surface(i32),
    Interface(i32),
    Readback,
}

use crate::capture::wgc::recording_owner::{CaptureFrameLease, ReleaseCaptureFrame};
use windows::Graphics::Capture::Direct3D11CaptureFrame;

impl ReleaseCaptureFrame for Direct3D11CaptureFrame {
    fn release_capture_frame(&self) {
        // Return the frame-pool slot after CPU readback, including every error path.
        let _ = self.Close();
    }
}

impl CaptureSession {
    pub(in crate::capture::wgc::windows) fn try_next_recording_frame(
        &mut self,
        device: &D3dDevice,
    ) -> Result<Option<ReadbackFrame>, RecordingFrameReadError> {
        let frame = match self.pool.TryGetNextFrame() {
            Ok(frame) => {
                self.poll_recovery.observe_success();
                frame
            }
            // A nullable empty pool is a normal pending state in the Rust WinRT projection.
            Err(error) if is_empty_frame_poll_hresult(error.code().0) => {
                self.poll_recovery.observe_success();
                return Ok(None);
            }
            Err(error) => {
                // Preserve the former adapter's one-second recovery budget for loaded VMs and
                // device transitions while keeping the empty-pool path non-blocking.
                wait_for_recording_frame_retry();
                if !self.poll_recovery.observe_failure() {
                    return Ok(None);
                }
                return Err(RecordingFrameReadError::Poll(error.code().0));
            }
        };
        let lease = CaptureFrameLease::new(frame);
        let frame = lease.frame();
        let system_relative_time_100ns = frame
            .SystemRelativeTime()
            .map_err(|error| RecordingFrameReadError::Timestamp(error.code().0))?
            .Duration;
        let surface = frame
            .Surface()
            .map_err(|error| RecordingFrameReadError::Surface(error.code().0))?;
        let texture = texture_from_surface(&surface).map_err(RecordingFrameReadError::Interface)?;
        let mut readback =
            readback_texture(device, &texture).map_err(|_| RecordingFrameReadError::Readback)?;
        readback.system_relative_time_100ns = system_relative_time_100ns;
        Ok(Some(readback))
    }
}
