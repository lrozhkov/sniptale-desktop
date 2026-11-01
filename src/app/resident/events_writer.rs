//! Event-specific persistence port over the shared process-owned bounded writer supervisor.

use super::history_writer::{HistoryWriterSink, WriterSlot, spawn_writer};
use crate::{
    protocol::recording_events::EventDescriptor,
    telemetry::{
        CursorTrajectorySinkError, RecordingEventsSink, RecordingEventsSinkFactory,
        RecordingTrajectoryContext,
    },
    transfer::events_store::EventsWriter,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Debug)]
pub(super) struct EventsWriterFactory {
    root: PathBuf,
    slot: Arc<Mutex<WriterSlot>>,
}

#[cfg(windows)]
impl EventsWriterFactory {
    pub(super) fn open_default() -> Option<Self> {
        use crate::transfer::events_store::default_events_root;
        use std::sync::OnceLock;
        static SLOT: OnceLock<Arc<Mutex<WriterSlot>>> = OnceLock::new();
        Some(Self {
            root: default_events_root().ok()?,
            slot: SLOT
                .get_or_init(|| Arc::new(Mutex::new(WriterSlot::default())))
                .clone(),
        })
    }
}

impl RecordingEventsSinkFactory for EventsWriterFactory {
    fn create(
        &mut self,
        recording: &str,
        stream: &str,
        context: &RecordingTrajectoryContext,
    ) -> Result<Box<dyn RecordingEventsSink>, CursorTrajectorySinkError> {
        Ok(Box::new(spawn_writer::<EventsWriter>(
            &self.root,
            &self.slot,
            recording,
            stream,
            context.delivery_route.clone(),
        )?))
    }
}

impl RecordingEventsSink for HistoryWriterSink<EventsWriter> {
    fn try_append(&mut self, bytes: Vec<u8>) -> Result<(), CursorTrajectorySinkError> {
        HistoryWriterSink::try_append(self, bytes)
    }
    fn finish(&mut self, incomplete: bool) -> EventDescriptor {
        HistoryWriterSink::finish(self, incomplete)
    }
    fn erase(&mut self) -> Result<(), CursorTrajectorySinkError> {
        HistoryWriterSink::erase(self)
    }
}

#[cfg(test)]
mod tests;
