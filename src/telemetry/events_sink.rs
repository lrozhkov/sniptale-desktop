//! Event-history persistence port; hook callbacks and their pump never wait for a disk queue.

use super::{CursorTrajectorySinkError, RecordingTrajectoryContext};
use crate::protocol::recording_events::EventDescriptor;

pub(crate) trait RecordingEventsSink: std::fmt::Debug + Send {
    fn try_append(&mut self, source_block: Vec<u8>) -> Result<(), CursorTrajectorySinkError>;
    fn finish(&mut self, incomplete: bool) -> EventDescriptor;
    fn erase(&mut self) -> Result<(), CursorTrajectorySinkError>;
}

pub(crate) trait RecordingEventsSinkFactory: std::fmt::Debug + Send {
    fn create(
        &mut self,
        recording: &str,
        stream: &str,
        context: &RecordingTrajectoryContext,
    ) -> Result<Box<dyn RecordingEventsSink>, CursorTrajectorySinkError>;
}
