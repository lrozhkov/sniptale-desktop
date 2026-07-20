use crate::recording::{
    encoder::{SinkWriterPlanError, windows_sink_writer::Mp4SinkWriterError},
    runtime::{RecordingRuntimeError, RecordingRuntimeFailure, worker::RecordingWorkerError},
};

use super::native_error::native_code;

pub(super) fn sink_plan_error(error: SinkWriterPlanError) -> RecordingRuntimeError {
    tracing::warn!(
        target: "sniptale_native_agent::recording",
        phase = "sink-plan",
        code = ?error,
        "recording runtime start failed"
    );
    RecordingRuntimeError::RuntimeFailure(RecordingRuntimeFailure::SinkPlan)
}

pub(super) fn sink_writer_create_error(error: Mp4SinkWriterError) -> RecordingRuntimeError {
    tracing::warn!(
        target: "sniptale_native_agent::recording",
        phase = "sink-writer-create",
        code = ?error,
        "recording runtime start failed"
    );
    RecordingRuntimeError::RuntimeFailure(RecordingRuntimeFailure::SinkCreate(native_code(error)))
}

pub(super) fn sink_writer_finalize_error(error: Mp4SinkWriterError) -> RecordingRuntimeError {
    tracing::warn!(
        target: "sniptale_native_agent::recording",
        phase = "sink-writer",
        code = ?error,
        "recording runtime failed"
    );
    RecordingRuntimeError::RuntimeFailure(RecordingRuntimeFailure::SinkFinalize(native_code(error)))
}

pub(super) fn worker_error(error: RecordingWorkerError) -> RecordingRuntimeError {
    tracing::warn!(
        target: "sniptale_native_agent::recording",
        phase = "media-pump",
        code = ?error,
        "recording runtime failed"
    );
    let failure = match error {
        RecordingWorkerError::VideoSource => RecordingRuntimeFailure::VideoSource,
        RecordingWorkerError::VideoPoll(code) => RecordingRuntimeFailure::VideoPoll(code),
        RecordingWorkerError::VideoTimestamp(code) => RecordingRuntimeFailure::VideoTimestamp(code),
        RecordingWorkerError::VideoSurface(code) => RecordingRuntimeFailure::VideoSurface(code),
        RecordingWorkerError::VideoInterface(code) => RecordingRuntimeFailure::VideoInterface(code),
        RecordingWorkerError::VideoReadback => RecordingRuntimeFailure::VideoReadback,
        RecordingWorkerError::VideoComposite => RecordingRuntimeFailure::VideoComposite,
        RecordingWorkerError::VideoGeometry => RecordingRuntimeFailure::VideoGeometry,
        RecordingWorkerError::VideoClock => RecordingRuntimeFailure::VideoClock,
        RecordingWorkerError::AudioSource => RecordingRuntimeFailure::AudioSource,
        RecordingWorkerError::Sink(_) => RecordingRuntimeFailure::SinkSample,
        RecordingWorkerError::SinkFailed => RecordingRuntimeFailure::SinkWrite(None),
        RecordingWorkerError::SinkNative(code) => RecordingRuntimeFailure::SinkWrite(Some(code)),
    };
    RecordingRuntimeError::RuntimeFailure(failure)
}
