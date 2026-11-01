//! Bounded, integer-only synchronous-distance reduction of normalized observations.

use crate::protocol::trajectory::{CursorTrajectoryEvent, CursorTrajectorySample};

pub(crate) const MAX_WINDOW_OBSERVATIONS: usize = 512;
pub(crate) const MAX_WINDOW_SPAN_US: u64 = 250_000;
const MAX_SAFE_INTEGER: u64 = (1_u64 << 53) - 1;
const MAX_FRAME_Q8: u64 = u32::MAX as u64 * 256 - 1;

/// Only the fields needed by SED; the caller retains the original canonical records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SedObservation {
    pub(crate) observation_seq: u64,
    pub(crate) segment_id: u64,
    pub(crate) active_time_us: u64,
    pub(crate) position_q8: Option<(u64, u64)>,
    pub(crate) visible: bool,
    pub(crate) button_event: bool,
    pub(crate) semantic_anchor: bool,
}

impl From<&CursorTrajectorySample> for SedObservation {
    fn from(value: &CursorTrajectorySample) -> Self {
        Self {
            observation_seq: value.observation_seq,
            segment_id: value.segment_id,
            active_time_us: value.active_time_us,
            position_q8: value.x_q8.zip(value.y_q8),
            visible: value.visible,
            button_event: value.event != CursorTrajectoryEvent::Move,
            semantic_anchor: !value.anchors.is_empty(),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrajectoryWindowError {
    InvalidObservation,
    TooManyObservations,
    SpanExceeded,
}

/// Returns source indices without changing timestamps, coordinates, or record metadata.
pub(crate) fn retained_indices(
    observations: &[SedObservation],
) -> Result<Vec<usize>, TrajectoryWindowError> {
    validate_window(observations)?;
    if observations.len() <= 2 {
        return Ok((0..observations.len()).collect());
    }
    let mut retained = mandatory_indices(observations);
    let anchors: Vec<_> = retained
        .iter()
        .enumerate()
        .filter_map(|(index, mandatory)| mandatory.then_some(index))
        .collect();
    let mut pending: Vec<_> = anchors.windows(2).map(|pair| (pair[0], pair[1])).collect();
    while let Some((start, end)) = pending.pop() {
        if end <= start + 1 {
            continue;
        }
        match interval_decision(observations, start, end) {
            IntervalDecision::KeepEndpoints => {}
            IntervalDecision::Split(index) => {
                retained[index] = true;
                pending.push((index, end));
                pending.push((start, index));
            }
            IntervalDecision::RetainAll => retained[start..=end].fill(true),
        }
    }
    Ok(retained
        .into_iter()
        .enumerate()
        .filter_map(|(index, keep)| keep.then_some(index))
        .collect())
}

fn validate_window(observations: &[SedObservation]) -> Result<(), TrajectoryWindowError> {
    if observations.len() > MAX_WINDOW_OBSERVATIONS {
        return Err(TrajectoryWindowError::TooManyObservations);
    }
    let Some(first) = observations.first() else {
        return Ok(());
    };
    for (index, point) in observations.iter().enumerate() {
        if point.observation_seq > MAX_SAFE_INTEGER
            || point.segment_id > MAX_SAFE_INTEGER
            || point.active_time_us > MAX_SAFE_INTEGER
            || point
                .position_q8
                .is_some_and(|(x, y)| x > MAX_FRAME_Q8 || y > MAX_FRAME_Q8)
            || index > 0
                && (point.observation_seq <= observations[index - 1].observation_seq
                    || point.active_time_us < observations[index - 1].active_time_us)
        {
            return Err(TrajectoryWindowError::InvalidObservation);
        }
        if point.active_time_us - first.active_time_us > MAX_WINDOW_SPAN_US {
            return Err(TrajectoryWindowError::SpanExceeded);
        }
    }
    Ok(())
}

fn mandatory_indices(observations: &[SedObservation]) -> Vec<bool> {
    let mut mandatory = vec![false; observations.len()];
    mandatory[0] = true;
    mandatory[observations.len() - 1] = true;
    let mut after_button = false;
    for (index, point) in observations.iter().enumerate() {
        if point.semantic_anchor || point.position_q8.is_none() {
            mandatory[index] = true;
        }
        if point.button_event {
            mandatory[index] = true;
            if index > 0 {
                mandatory[index - 1] = true;
            }
            after_button = true;
        } else if after_button {
            mandatory[index] = true;
            after_button = false;
        }
        if index > 0 {
            let previous = &observations[index - 1];
            if point.segment_id != previous.segment_id
                || point.visible != previous.visible
                || point.position_q8.is_some() != previous.position_q8.is_some()
                || point.active_time_us == previous.active_time_us
            {
                mandatory[index - 1] = true;
                mandatory[index] = true;
            }
        }
    }
    mandatory
}

enum IntervalDecision {
    KeepEndpoints,
    Split(usize),
    RetainAll,
}

fn interval_decision(
    observations: &[SedObservation],
    start: usize,
    end: usize,
) -> IntervalDecision {
    let a = observations[start];
    let b = observations[end];
    let Some(duration) = b.active_time_us.checked_sub(a.active_time_us) else {
        return IntervalDecision::RetainAll;
    };
    if duration == 0 || a.position_q8.is_none() || b.position_q8.is_none() {
        return IntervalDecision::RetainAll;
    }
    let mut maximum_error = 0;
    let mut split = start + 1;
    for (index, point) in observations.iter().enumerate().take(end).skip(start + 1) {
        let Some(error) = squared_numerator(a, b, *point, duration) else {
            return IntervalDecision::RetainAll;
        };
        // Input sequence increases strictly, so the first equal maximum has the lower seq.
        if error > maximum_error {
            maximum_error = error;
            split = index;
        }
    }
    let duration = u128::from(duration);
    let threshold = 50_944_u128
        .checked_mul(50_944)
        .and_then(|value| value.checked_mul(duration))
        .and_then(|value| value.checked_mul(duration));
    let Some(threshold) = threshold else {
        return IntervalDecision::RetainAll;
    };
    // e * 10000 > R is exactly e > floor(R / 10000), avoiding a large product.
    if maximum_error > threshold / 10_000 {
        IntervalDecision::Split(split)
    } else {
        IntervalDecision::KeepEndpoints
    }
}

fn squared_numerator(
    a: SedObservation,
    b: SedObservation,
    point: SedObservation,
    duration: u64,
) -> Option<u128> {
    let (ax, ay) = a.position_q8?;
    let (bx, by) = b.position_q8?;
    let (px, py) = point.position_q8?;
    let elapsed = i128::from(point.active_time_us.checked_sub(a.active_time_us)?);
    let duration = i128::from(duration);
    let numerator = |origin: u64, end: u64, value: u64| {
        (i128::from(value) - i128::from(origin))
            .checked_mul(duration)?
            .checked_sub((i128::from(end) - i128::from(origin)).checked_mul(elapsed)?)
            .map(i128::unsigned_abs)
    };
    let nx = numerator(ax, bx, px)?;
    let ny = numerator(ay, by, py)?;
    nx.checked_mul(nx)?.checked_add(ny.checked_mul(ny)?)
}
