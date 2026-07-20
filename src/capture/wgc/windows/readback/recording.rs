//! Persistent recording frame polling and typed WGC diagnostics.

use windows::core::HRESULT;

use super::{
    CaptureSession, D3dDevice, ReadbackFrame, readback_texture, texture_from_surface,
    wait_for_recording_frame_retry,
};

const NO_FRAME_HRESULT: HRESULT = HRESULT(0x8000_4003_u32 as i32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::capture::wgc::windows) enum RecordingFrameReadError {
    Poll(i32),
    Timestamp(i32),
    Surface(i32),
    Interface(i32),
    Readback,
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
            // A documented null poll is projected as E_POINTER; the session remains live.
            Err(error) if error.code() == NO_FRAME_HRESULT => {
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
