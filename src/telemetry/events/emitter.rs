//! One bounded packing buffer and a nonblocking persistence port, with no lifetime history array.

use crate::protocol::{
    recording_events::{
        EventAlgorithm, EventDescriptor, EventEncoding, EventFormat, EventPacker, EventRecord,
    },
    trajectory::CursorTrajectoryState,
};
use crate::telemetry::{CursorTrajectorySinkError, RecordingEventsSink};

#[derive(Debug)]
pub(super) struct EventEmitter {
    pub(super) sequence: u64,
    pub(super) segment: u64,
    pub(super) failed: bool,
    packer: EventPacker,
    sink: Option<Box<dyn RecordingEventsSink>>,
    descriptor: EventDescriptor,
}

impl EventEmitter {
    pub(super) fn new(
        recording: String,
        source: String,
        sink: Option<Box<dyn RecordingEventsSink>>,
    ) -> Self {
        let available = sink.is_some();
        Self {
            sequence: 0,
            segment: 0,
            failed: !available,
            packer: EventPacker::new(recording, source.clone()),
            sink,
            descriptor: EventDescriptor {
                format: EventFormat::V1,
                format_version: 1,
                algorithm: EventAlgorithm::LosslessV1,
                encoding: EventEncoding::JsonUtf8V1,
                event_stream_id: source,
                state: if available {
                    CursorTrajectoryState::Complete
                } else {
                    CursorTrajectoryState::Unavailable
                },
                finalized: !available,
                total_blocks: 0,
                total_records: 0,
                total_bytes: 0,
                stream_sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
                    .to_owned(),
            },
        }
    }

    pub(super) fn id(&self, prefix: &str) -> String {
        format!("native-{prefix}-{}", self.sequence)
    }

    pub(super) fn emit(
        &mut self,
        record: impl FnOnce(u64, u64) -> EventRecord,
    ) -> Result<(), CursorTrajectorySinkError> {
        if self.failed {
            return Err(CursorTrajectorySinkError::Retired);
        }
        let output = self
            .packer
            .push(record(self.sequence, self.segment))
            .map_err(|_| {
                self.failed = true;
                CursorTrajectorySinkError::Storage
            })?;
        self.sequence = self.sequence.checked_add(1).ok_or_else(|| {
            self.failed = true;
            CursorTrajectorySinkError::Storage
        })?;
        self.send(output)
    }

    fn send(&mut self, blocks: Vec<Vec<u8>>) -> Result<(), CursorTrajectorySinkError> {
        let sink = self
            .sink
            .as_mut()
            .ok_or(CursorTrajectorySinkError::Storage)?;
        for block in blocks {
            if let Err(error) = sink.try_append(block) {
                self.failed = true;
                return Err(error);
            }
        }
        Ok(())
    }

    pub(super) fn finish(&mut self) -> EventDescriptor {
        if !self.failed {
            match self.packer.finish() {
                Ok(blocks) => {
                    let _ = self.send(blocks);
                }
                Err(_) => self.failed = true,
            }
        }
        if let Some(sink) = self.sink.as_mut() {
            self.descriptor = sink.finish(self.failed);
        }
        self.descriptor.clone()
    }

    pub(super) fn erase(&mut self) -> Result<(), CursorTrajectorySinkError> {
        self.failed = true;
        if let Some(sink) = self.sink.as_mut() {
            sink.erase()?;
        }
        Ok(())
    }

    pub(super) fn descriptor(&self) -> &EventDescriptor {
        &self.descriptor
    }
}
