//! Cursor-specific factory and telemetry port over shared process-owned history supervision.

use super::history_writer::{HistoryWriterSink, WriterSlot, spawn_writer};
use crate::{
    protocol::trajectory::CursorTrajectoryDescriptor,
    telemetry::{
        CursorTrajectorySink, CursorTrajectorySinkError, CursorTrajectorySinkFactory,
        RecordingTrajectoryContext,
    },
    transfer::trajectory_store::TrajectoryWriter,
};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

#[derive(Debug)]
pub(super) struct CursorWriterFactory {
    root: PathBuf,
    slot: Arc<Mutex<WriterSlot>>,
}

#[cfg(windows)]
impl CursorWriterFactory {
    pub(super) fn open_default() -> Option<Self> {
        use crate::transfer::trajectory_store::default_trajectory_root;
        use std::sync::OnceLock;
        static SLOT: OnceLock<Arc<Mutex<WriterSlot>>> = OnceLock::new();
        Some(Self {
            root: default_trajectory_root().ok()?,
            slot: SLOT
                .get_or_init(|| Arc::new(Mutex::new(WriterSlot::default())))
                .clone(),
        })
    }
}

type CursorWriterSink = HistoryWriterSink<TrajectoryWriter>;

impl CursorTrajectorySinkFactory for CursorWriterFactory {
    fn create(
        &mut self,
        recording_id: &str,
        trajectory_id: &str,
        context: &RecordingTrajectoryContext,
    ) -> Result<Box<dyn CursorTrajectorySink>, CursorTrajectorySinkError> {
        Ok(Box::new(spawn_writer::<TrajectoryWriter>(
            &self.root,
            &self.slot,
            recording_id,
            trajectory_id,
            context.delivery_route.clone(),
        )?))
    }
}

impl CursorTrajectorySink for CursorWriterSink {
    fn try_append(&mut self, bytes: Vec<u8>) -> Result<(), CursorTrajectorySinkError> {
        HistoryWriterSink::try_append(self, bytes)
    }
    fn finish(&mut self, incomplete: bool) -> CursorTrajectoryDescriptor {
        HistoryWriterSink::finish(self, incomplete)
    }
    fn erase(&mut self) -> Result<(), CursorTrajectorySinkError> {
        HistoryWriterSink::erase(self)
    }
}

#[cfg(test)]
use super::history_writer::{FINALIZE_INCOMPLETE, WriterStatus};
#[cfg(test)]
use crate::transfer::trajectory_store::TrajectoryArchive;
#[cfg(test)]
use std::sync::{
    Condvar,
    atomic::{AtomicU8, Ordering},
    mpsc,
};
#[cfg(test)]
mod tests;
