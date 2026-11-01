//! A bounded source-driven window retaining original records and immutable output prefixes.

use super::trajectory::{
    MAX_WINDOW_OBSERVATIONS, MAX_WINDOW_SPAN_US, SedObservation, TrajectoryWindowError,
    retained_indices,
};
use crate::protocol::trajectory::{
    CursorTrajectoryRecord, CursorTrajectorySample, validate_record_shape,
};

#[derive(Debug, Default)]
pub(crate) struct CursorTrajectoryWindow {
    pending: Vec<CursorTrajectorySample>,
    previous: Option<CursorTrajectorySample>,
    emitted_sequence: Option<u64>,
}

impl CursorTrajectoryWindow {
    pub(crate) fn append(
        &mut self,
        observation: CursorTrajectorySample,
    ) -> Result<Vec<CursorTrajectorySample>, TrajectoryWindowError> {
        validate_record_shape(&CursorTrajectoryRecord::Sample(observation.clone()))
            .map_err(|_| TrajectoryWindowError::InvalidObservation)?;
        if self.previous.as_ref().is_some_and(|previous| {
            observation.observation_seq <= previous.observation_seq
                || observation.active_time_us < previous.active_time_us
        }) {
            return Err(TrajectoryWindowError::InvalidObservation);
        }
        let discontinuity = self.previous.as_ref().is_some_and(|previous| {
            observation.active_time_us == previous.active_time_us
                || observation.segment_id != previous.segment_id
                || observation.observed != previous.observed
                || observation.visible != previous.visible
        });
        let span_exceeded = self.pending.first().is_some_and(|first| {
            observation.active_time_us - first.active_time_us > MAX_WINDOW_SPAN_US
        });
        let mut output = Vec::new();
        if discontinuity || span_exceeded {
            output = self.flush()?;
            if !discontinuity
                && let Some(previous) = self.previous.as_ref()
                && observation.active_time_us - previous.active_time_us <= MAX_WINDOW_SPAN_US
            {
                self.pending.push(previous.clone());
            }
        }
        self.previous = Some(observation.clone());
        self.pending.push(observation);
        if self.pending.len() == MAX_WINDOW_OBSERVATIONS {
            output.extend(self.flush()?);
            if let Some(previous) = &self.previous {
                self.pending.push(previous.clone());
            }
        }
        Ok(output)
    }

    /// Explicit lifecycle/gap flush. A subsequent segment cannot interpolate across it.
    pub(crate) fn flush(&mut self) -> Result<Vec<CursorTrajectorySample>, TrajectoryWindowError> {
        let points: Vec<_> = self.pending.iter().map(SedObservation::from).collect();
        let indices = retained_indices(&points)?;
        let output: Vec<_> = indices
            .into_iter()
            .map(|index| &self.pending[index])
            .filter(|point| {
                self.emitted_sequence
                    .is_none_or(|sequence| point.observation_seq > sequence)
            })
            .cloned()
            .collect();
        if let Some(last) = output.last() {
            self.emitted_sequence = Some(last.observation_seq);
        }
        self.pending.clear();
        Ok(output)
    }

    #[cfg(test)]
    pub(crate) fn pending_count(&self) -> usize {
        self.pending.len()
    }
}
