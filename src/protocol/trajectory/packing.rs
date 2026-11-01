//! Deterministic greedy packing; source windows and I/O scheduling never seal blocks.

use super::{
    canonical::canonical_bytes,
    records::{CursorTrajectoryBlock, CursorTrajectoryRecord},
    validation::{
        MAX_BLOCK_BYTES, MAX_BLOCK_RECORDS, TrajectoryContractError, is_trajectory_barrier,
        record_bounds, safe, valid_id, valid_trajectory_id, validate_record_shape,
    },
};

#[derive(Debug)]
pub(crate) struct CursorTrajectoryPacker {
    recording_id: String,
    trajectory_id: String,
    sequence: u64,
    start_time_us: u64,
    end_time_us: u64,
    records: Vec<Vec<u8>>,
    record_bytes: usize,
    closed: bool,
}

impl CursorTrajectoryPacker {
    pub(crate) fn new(
        recording_id: String,
        trajectory_id: String,
    ) -> Result<Self, TrajectoryContractError> {
        if !valid_id(&recording_id) || !valid_trajectory_id(&trajectory_id) {
            return Err(TrajectoryContractError::Identifier);
        }
        Ok(Self {
            recording_id,
            trajectory_id,
            sequence: 0,
            start_time_us: 0,
            end_time_us: 0,
            records: Vec::new(),
            record_bytes: 0,
            closed: false,
        })
    }

    /// One input can seal at most two blocks: a full prefix, then a boundary/gap.
    pub(crate) fn append(
        &mut self,
        record: &CursorTrajectoryRecord,
    ) -> Result<Vec<Vec<u8>>, TrajectoryContractError> {
        if self.closed {
            return Err(TrajectoryContractError::Order);
        }
        validate_record_shape(record)?;
        let bytes = canonical_bytes(record)?;
        let (start, end) = record_bounds(record);
        let mut completed = Vec::new();
        if !self.fits(bytes.len(), start, end)? {
            if self.records.is_empty() {
                return Err(TrajectoryContractError::Oversize);
            }
            completed.push(self.seal()?.ok_or(TrajectoryContractError::Count)?);
            if !self.fits(bytes.len(), start, end)? {
                return Err(TrajectoryContractError::Oversize);
            }
        }
        self.start_time_us = if self.records.is_empty() {
            start
        } else {
            self.start_time_us.min(start)
        };
        self.end_time_us = if self.records.is_empty() {
            end
        } else {
            self.end_time_us.max(end)
        };
        self.record_bytes += bytes.len();
        self.records.push(bytes);
        if is_trajectory_barrier(record) {
            completed.push(self.seal()?.ok_or(TrajectoryContractError::Count)?);
        }
        Ok(completed)
    }

    pub(crate) fn finish(&mut self) -> Result<Option<Vec<u8>>, TrajectoryContractError> {
        self.closed = true;
        self.seal()
    }

    fn fits(&self, bytes: usize, start: u64, end: u64) -> Result<bool, TrajectoryContractError> {
        if self.records.len() >= MAX_BLOCK_RECORDS {
            return Ok(false);
        }
        let start = if self.records.is_empty() {
            start
        } else {
            start.min(self.start_time_us)
        };
        let end = if self.records.is_empty() {
            end
        } else {
            end.max(self.end_time_us)
        };
        let header = self.header(start, end)?;
        Ok(header.len() + self.record_bytes + bytes + self.records.len() <= MAX_BLOCK_BYTES)
    }

    fn header(&self, start: u64, end: u64) -> Result<Vec<u8>, TrajectoryContractError> {
        canonical_bytes(&CursorTrajectoryBlock {
            format_version: 1,
            recording_id: self.recording_id.clone(),
            trajectory_id: self.trajectory_id.clone(),
            sequence: self.sequence,
            start_time_us: start,
            end_time_us: end,
            records: Vec::new(),
        })
    }

    fn seal(&mut self) -> Result<Option<Vec<u8>>, TrajectoryContractError> {
        if self.records.is_empty() {
            return Ok(None);
        }
        let next_sequence = self
            .sequence
            .checked_add(1)
            .filter(|value| safe(*value))
            .ok_or(TrajectoryContractError::Number)?;
        let header = self.header(self.start_time_us, self.end_time_us)?;
        // This marker is created by the closed header serializer, never supplied by input text.
        let marker = b"\"records\":[]";
        let position = header
            .windows(marker.len())
            .position(|bytes| bytes == marker)
            .ok_or(TrajectoryContractError::Malformed)?;
        let insertion = position + marker.len() - 1;
        let mut bytes =
            Vec::with_capacity(header.len() + self.record_bytes + self.records.len() - 1);
        bytes.extend_from_slice(&header[..insertion]);
        for (index, record) in self.records.iter().enumerate() {
            if index > 0 {
                bytes.push(b',');
            }
            bytes.extend_from_slice(record);
        }
        bytes.extend_from_slice(&header[insertion..]);
        if bytes.len() > MAX_BLOCK_BYTES {
            return Err(TrajectoryContractError::Oversize);
        }
        self.records.clear();
        self.record_bytes = 0;
        self.sequence = next_sequence;
        Ok(Some(bytes))
    }
}
