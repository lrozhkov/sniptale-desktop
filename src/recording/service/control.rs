//! Recording control command mapping and runtime signaling.

use super::super::{
    runtime::{RecordingRuntime, RecordingRuntimeError},
    session::RecordingControl as SessionRecordingControl,
};
use crate::protocol::RecordingControl;

pub(super) const fn recording_control(control: &RecordingControl) -> SessionRecordingControl {
    match control {
        RecordingControl::Pause => SessionRecordingControl::Pause,
        RecordingControl::Resume => SessionRecordingControl::Resume,
        RecordingControl::Stop => SessionRecordingControl::Stop,
        RecordingControl::Cancel => SessionRecordingControl::Cancel,
    }
}

pub(super) fn signal_runtime_control<R>(
    runtime: &mut R,
    control: SessionRecordingControl,
    recording_id: &str,
) -> Result<(), RecordingRuntimeError>
where
    R: RecordingRuntime,
{
    match control {
        SessionRecordingControl::Pause => runtime.pause_recording(recording_id),
        SessionRecordingControl::Resume => runtime.resume_recording(recording_id),
        SessionRecordingControl::Stop | SessionRecordingControl::Cancel => Ok(()),
    }
}
