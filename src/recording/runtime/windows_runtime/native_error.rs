//! Native Media Foundation error classification without logging side effects.

use crate::recording::encoder::windows_sink_writer::Mp4SinkWriterError;

pub(super) const fn native_code(error: Mp4SinkWriterError) -> Option<i32> {
    match error {
        Mp4SinkWriterError::Windows(code) => Some(code),
        Mp4SinkWriterError::PathContainsNul
        | Mp4SinkWriterError::InvalidCodecValue
        | Mp4SinkWriterError::InvalidSample(_) => None,
    }
}
