//! Source-driven segmentation, bounded SED windows, and immutable block production.

use super::trajectory_window::CursorTrajectoryWindow;
use crate::protocol::trajectory::{
    CursorTrajectoryBoundary, CursorTrajectoryBoundaryReason, CursorTrajectoryClock,
    CursorTrajectoryGap, CursorTrajectoryPacker, CursorTrajectoryRecord, CursorTrajectorySample,
    CursorTrajectorySegment, CursorTrajectorySegmentReason, MAX_SAFE_INTEGER,
    TrajectoryContractError, validate_record_shape,
};

#[derive(Debug)]
pub(crate) struct CursorTrajectoryProducer {
    packer: CursorTrajectoryPacker,
    window: CursorTrajectoryWindow,
    geometry: (u32, u32),
    clock: CursorTrajectoryClock,
    segment: Option<u64>,
    last_segment: u64,
    previous: Option<CursorTrajectorySample>,
    observation_floor: Option<u64>,
    time_floor: u64,
    incomplete: bool,
    closed: bool,
}

impl CursorTrajectoryProducer {
    pub(crate) fn new(
        recording_id: String,
        trajectory_id: String,
        initial: CursorTrajectorySegment,
    ) -> Result<Self, TrajectoryContractError> {
        if initial.segment_id != 0 || initial.reason != CursorTrajectorySegmentReason::Start {
            return Err(TrajectoryContractError::Order);
        }
        let mut packer = CursorTrajectoryPacker::new(recording_id, trajectory_id)?;
        let geometry = (initial.width, initial.height);
        let clock = initial.clock;
        let time_floor = initial.active_time_us;
        packer.append(&CursorTrajectoryRecord::Segment(initial))?;
        Ok(Self {
            packer,
            window: CursorTrajectoryWindow::default(),
            geometry,
            clock,
            segment: Some(0),
            last_segment: 0,
            previous: None,
            observation_floor: None,
            time_floor,
            incomplete: false,
            closed: false,
        })
    }

    pub(crate) fn observe(
        &mut self,
        mut sample: CursorTrajectorySample,
    ) -> Result<Vec<Vec<u8>>, TrajectoryContractError> {
        self.require_time(sample.active_time_us)?;
        validate_record_shape(&CursorTrajectoryRecord::Sample(sample.clone()))?;
        if self
            .observation_floor
            .is_some_and(|last| sample.observation_seq <= last)
        {
            return Err(TrajectoryContractError::Order);
        }
        if sample
            .x_q8
            .is_some_and(|x| x >= u64::from(self.geometry.0) * 256)
            || sample
                .y_q8
                .is_some_and(|y| y >= u64::from(self.geometry.1) * 256)
        {
            return Err(TrajectoryContractError::Coordinates);
        }
        let mut blocks = Vec::new();
        if let Some((boundary, reason)) = self.discontinuity(&sample) {
            blocks.extend(self.boundary(sample.active_time_us, boundary)?);
            blocks.extend(self.begin_segment(
                sample.active_time_us,
                self.geometry,
                self.clock,
                reason,
            )?);
        }
        sample.segment_id = self.segment.ok_or(TrajectoryContractError::Order)?;
        let retained = self
            .window
            .append(sample.clone())
            .map_err(|_| TrajectoryContractError::Order)?;
        for point in retained {
            blocks.extend(self.packer.append(&CursorTrajectoryRecord::Sample(point))?);
        }
        self.observation_floor = Some(sample.observation_seq);
        self.time_floor = sample.active_time_us;
        self.previous = Some(sample);
        Ok(blocks)
    }

    pub(crate) fn begin_segment(
        &mut self,
        time: u64,
        geometry: (u32, u32),
        clock: CursorTrajectoryClock,
        reason: CursorTrajectorySegmentReason,
    ) -> Result<Vec<Vec<u8>>, TrajectoryContractError> {
        self.require_time(time)?;
        if self.segment.is_some() {
            return Err(TrajectoryContractError::Order);
        }
        let id = self
            .last_segment
            .checked_add(1)
            .filter(|id| *id <= MAX_SAFE_INTEGER)
            .ok_or(TrajectoryContractError::Number)?;
        let record = CursorTrajectoryRecord::Segment(CursorTrajectorySegment {
            segment_id: id,
            active_time_us: time,
            width: geometry.0,
            height: geometry.1,
            clock,
            reason,
        });
        let blocks = self.packer.append(&record)?;
        self.segment = Some(id);
        self.last_segment = id;
        self.geometry = geometry;
        self.clock = clock;
        self.previous = None;
        self.time_floor = time;
        Ok(blocks)
    }

    pub(crate) fn boundary(
        &mut self,
        time: u64,
        reason: CursorTrajectoryBoundaryReason,
    ) -> Result<Vec<Vec<u8>>, TrajectoryContractError> {
        self.require_time(time)?;
        let id = self.segment.ok_or(TrajectoryContractError::Order)?;
        let mut blocks = self.flush_window()?;
        blocks.extend(self.packer.append(&CursorTrajectoryRecord::Boundary(
            CursorTrajectoryBoundary {
                segment_id: id,
                active_time_us: time,
                reason,
            },
        ))?);
        self.segment = None;
        self.previous = None;
        self.time_floor = time;
        if reason == CursorTrajectoryBoundaryReason::Stop {
            self.closed = true;
        }
        Ok(blocks)
    }

    pub(crate) fn gap(
        &mut self,
        mut gap: CursorTrajectoryGap,
    ) -> Result<Vec<Vec<u8>>, TrajectoryContractError> {
        self.require_time(gap.from_time_us)?;
        gap.segment_id = self.segment.ok_or(TrajectoryContractError::Order)?;
        validate_record_shape(&CursorTrajectoryRecord::Gap(gap.clone()))?;
        if gap
            .first_lost_observation_seq
            .is_some_and(|first| self.observation_floor.is_some_and(|last| first <= last))
        {
            return Err(TrajectoryContractError::Order);
        }
        let mut blocks = self.flush_window()?;
        blocks.extend(
            self.packer
                .append(&CursorTrajectoryRecord::Gap(gap.clone()))?,
        );
        if gap.last_lost_observation_seq.is_some() {
            self.observation_floor = gap.last_lost_observation_seq;
        }
        self.time_floor = gap.to_time_us.unwrap_or(gap.from_time_us);
        self.segment = None;
        self.previous = None;
        self.incomplete = true;
        Ok(blocks)
    }

    pub(crate) fn finish(&mut self) -> Result<Vec<Vec<u8>>, TrajectoryContractError> {
        let mut blocks = self.flush_window()?;
        if let Some(block) = self.packer.finish()? {
            blocks.push(block);
        }
        self.closed = true;
        Ok(blocks)
    }

    pub(crate) fn incomplete(&self) -> bool {
        self.incomplete
    }

    #[cfg(test)]
    pub(crate) fn pending_observations(&self) -> usize {
        self.window.pending_count()
    }

    fn require_time(&self, time: u64) -> Result<(), TrajectoryContractError> {
        if self.closed || time < self.time_floor || time > MAX_SAFE_INTEGER {
            Err(TrajectoryContractError::Order)
        } else {
            Ok(())
        }
    }

    fn flush_window(&mut self) -> Result<Vec<Vec<u8>>, TrajectoryContractError> {
        let mut blocks = Vec::new();
        for sample in self
            .window
            .flush()
            .map_err(|_| TrajectoryContractError::Order)?
        {
            blocks.extend(
                self.packer
                    .append(&CursorTrajectoryRecord::Sample(sample))?,
            );
        }
        Ok(blocks)
    }

    fn discontinuity(
        &self,
        sample: &CursorTrajectorySample,
    ) -> Option<(
        CursorTrajectoryBoundaryReason,
        CursorTrajectorySegmentReason,
    )> {
        let previous = self.previous.as_ref()?;
        if sample.observed != previous.observed {
            Some((
                CursorTrajectoryBoundaryReason::Observation,
                CursorTrajectorySegmentReason::Observation,
            ))
        } else if sample.visible != previous.visible {
            Some((
                CursorTrajectoryBoundaryReason::Visibility,
                CursorTrajectorySegmentReason::Visibility,
            ))
        } else if sample.active_time_us == previous.active_time_us
            && (sample.x_q8, sample.y_q8) != (previous.x_q8, previous.y_q8)
        {
            Some((
                CursorTrajectoryBoundaryReason::ZeroTimeDiscontinuity,
                CursorTrajectorySegmentReason::ZeroTimeDiscontinuity,
            ))
        } else {
            None
        }
    }
}
