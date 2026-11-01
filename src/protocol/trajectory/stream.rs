//! Constant-memory validation of geometry and ordering across immutable blocks.

use super::{
    CursorTrajectoryBlock, CursorTrajectoryBoundaryReason, CursorTrajectoryRecord,
    CursorTrajectorySample, CursorTrajectorySegment, TrajectoryContractError,
    validation::validate_block_shape,
};

#[derive(Clone, Debug, Default)]
pub(crate) struct CursorTrajectoryStreamValidator {
    binding: Option<(String, String)>,
    next_sequence: u64,
    last_segment: Option<u64>,
    segment: Option<CursorTrajectorySegment>,
    last_sample: Option<CursorTrajectorySample>,
    observation_floor: Option<u64>,
    time_floor: u64,
    stopped: bool,
    incomplete: bool,
}

impl CursorTrajectoryStreamValidator {
    /// Commit validation state only when the whole block is valid.
    pub(crate) fn accept(
        &mut self,
        block: &CursorTrajectoryBlock,
    ) -> Result<(), TrajectoryContractError> {
        validate_block_shape(block)?;
        if self.stopped || block.sequence != self.next_sequence {
            return Err(TrajectoryContractError::Order);
        }
        if self
            .binding
            .as_ref()
            .is_some_and(|(recording, trajectory)| {
                *recording != block.recording_id || *trajectory != block.trajectory_id
            })
        {
            return Err(TrajectoryContractError::Identifier);
        }
        let mut candidate = self.clone();
        candidate
            .binding
            .get_or_insert_with(|| (block.recording_id.clone(), block.trajectory_id.clone()));
        for record in &block.records {
            candidate.accept_record(record)?;
        }
        candidate.next_sequence += 1;
        *self = candidate;
        Ok(())
    }

    pub(crate) fn incomplete(&self) -> bool {
        self.incomplete
    }

    #[cfg(test)]
    pub(crate) fn stopped(&self) -> bool {
        self.stopped
    }

    pub(crate) fn closed_prefix(&self) -> bool {
        self.segment.is_none() && self.last_segment.is_some()
    }

    fn accept_record(
        &mut self,
        record: &CursorTrajectoryRecord,
    ) -> Result<(), TrajectoryContractError> {
        let (start, end) = super::record_bounds(record);
        if start < self.time_floor {
            return Err(TrajectoryContractError::Order);
        }
        match record {
            CursorTrajectoryRecord::Segment(segment) => {
                if self.segment.is_some()
                    || self
                        .last_segment
                        .map_or(segment.segment_id != 0, |last| segment.segment_id <= last)
                {
                    return Err(TrajectoryContractError::Order);
                }
                self.segment = Some(segment.clone());
                self.last_segment = Some(segment.segment_id);
                self.last_sample = None;
            }
            CursorTrajectoryRecord::Sample(sample) => self.accept_sample(sample)?,
            CursorTrajectoryRecord::Boundary(boundary) => {
                self.require_segment(boundary.segment_id)?;
                self.segment = None;
                self.last_sample = None;
                self.stopped = boundary.reason == CursorTrajectoryBoundaryReason::Stop;
            }
            CursorTrajectoryRecord::Gap(gap) => {
                self.require_segment(gap.segment_id)?;
                if let Some(first) = gap.first_lost_observation_seq {
                    if self.observation_floor.is_some_and(|last| first <= last) {
                        return Err(TrajectoryContractError::Order);
                    }
                    self.observation_floor = gap.last_lost_observation_seq;
                }
                self.segment = None;
                self.last_sample = None;
                self.incomplete = true;
            }
        }
        self.time_floor = end;
        Ok(())
    }

    fn require_segment(
        &self,
        id: u64,
    ) -> Result<&CursorTrajectorySegment, TrajectoryContractError> {
        self.segment
            .as_ref()
            .filter(|segment| segment.segment_id == id)
            .ok_or(TrajectoryContractError::Order)
    }

    fn accept_sample(
        &mut self,
        sample: &CursorTrajectorySample,
    ) -> Result<(), TrajectoryContractError> {
        let segment = self.require_segment(sample.segment_id)?;
        if sample
            .x_q8
            .is_some_and(|x| x >= u64::from(segment.width) * 256)
            || sample
                .y_q8
                .is_some_and(|y| y >= u64::from(segment.height) * 256)
        {
            return Err(TrajectoryContractError::Coordinates);
        }
        if self
            .observation_floor
            .is_some_and(|last| sample.observation_seq <= last)
        {
            return Err(TrajectoryContractError::Order);
        }
        if self.last_sample.as_ref().is_some_and(|last| {
            last.visible != sample.visible
                || last.observed != sample.observed
                || (last.active_time_us == sample.active_time_us
                    && (last.x_q8, last.y_q8) != (sample.x_q8, sample.y_q8))
        }) {
            return Err(TrajectoryContractError::Order);
        }
        self.observation_floor = Some(sample.observation_seq);
        self.last_sample = Some(sample.clone());
        Ok(())
    }
}
