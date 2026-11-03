//! Recording cursor lifecycle; the sink owns disk work and committed-prefix authority.

use crate::protocol::trajectory::{
    CursorTrajectoryAlgorithm, CursorTrajectoryBoundaryReason, CursorTrajectoryButton,
    CursorTrajectoryDescriptor, CursorTrajectoryEncoding, CursorTrajectoryEvent,
    CursorTrajectoryFormat, CursorTrajectoryGap, CursorTrajectoryGapReason, CursorTrajectorySample,
    CursorTrajectorySegment, CursorTrajectorySegmentReason, CursorTrajectoryState,
};
use crate::telemetry::{
    CursorTrajectorySink, CursorTrajectorySinkFactory, RecordingTrajectoryContext,
    trajectory_clock::{TrajectoryActiveClock, TrajectoryClockError, WindowsTrajectoryClock},
    trajectory_producer::CursorTrajectoryProducer,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Capturing,
    Paused,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CursorRuntimeError {
    Clock,
    Contract,
    Sink,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct CursorSourceObservation {
    pub(super) visible: Option<bool>,
    pub(super) sequence: u64,
    pub(super) source_time_ms: u64,
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) event: CursorTrajectoryEvent,
    pub(super) button: CursorTrajectoryButton,
}

#[derive(Debug)]
pub(super) struct RuntimeCursorTrajectory {
    producer: Option<CursorTrajectoryProducer>,
    sink: Option<Box<dyn CursorTrajectorySink>>,
    descriptor: CursorTrajectoryDescriptor,
    context: RecordingTrajectoryContext,
    source_clock: Option<WindowsTrajectoryClock>,
    active_clock: TrajectoryActiveClock,
    raw_sequence: Option<u64>,
    last_time: u64,
    pending_segment: Option<CursorTrajectorySegmentReason>,
    phase: Phase,
    failed: bool,
}

impl RuntimeCursorTrajectory {
    pub(super) fn fail(&mut self) {
        self.failed = true;
    }
    pub(super) fn new(
        recording: &str,
        trajectory: String,
        context: RecordingTrajectoryContext,
        started_ns: u64,
        clock: Option<WindowsTrajectoryClock>,
        factory: Option<&mut dyn CursorTrajectorySinkFactory>,
    ) -> Self {
        let initial = CursorTrajectorySegment {
            segment_id: 0,
            active_time_us: 0,
            width: context.frame.width,
            height: context.frame.height,
            clock: WindowsTrajectoryClock::metadata(),
            reason: CursorTrajectorySegmentReason::Start,
        };
        let prepared =
            CursorTrajectoryProducer::new(recording.to_owned(), trajectory.clone(), initial);
        let (producer, sink) = match (prepared, factory, clock, context.point_mapping) {
            (
                Ok(producer),
                Some(factory),
                Some(_),
                crate::telemetry::RecordingPointMapping::FixedDesktopFrame,
            ) => match factory.create(recording, &trajectory, &context) {
                Ok(sink) => (Some(producer), Some(sink)),
                _ => (None, None),
            },
            _ => (None, None),
        };
        Self {
            producer,
            sink,
            descriptor: unavailable(trajectory),
            context,
            source_clock: clock,
            active_clock: TrajectoryActiveClock::new(started_ns),
            raw_sequence: None,
            last_time: 0,
            pending_segment: None,
            phase: Phase::Capturing,
            failed: false,
        }
    }

    pub(super) fn observe(
        &mut self,
        observation: CursorSourceObservation,
    ) -> Result<(), CursorRuntimeError> {
        if self.failed || self.phase != Phase::Capturing || self.producer.is_none() {
            self.raw_sequence = Some(observation.sequence);
            return Ok(());
        }
        let source_ns = self
            .source_clock
            .ok_or(CursorRuntimeError::Clock)?
            .event_monotonic_ns(observation.source_time_ms)
            .map_err(|_| CursorRuntimeError::Clock)?;
        let time = match self.active_clock.observe(source_ns) {
            Ok(time) => time,
            Err(TrajectoryClockError::BeforeStart) => {
                self.raw_sequence = Some(observation.sequence);
                return Ok(());
            }
            Err(_) => {
                self.failed = true;
                return Err(CursorRuntimeError::Clock);
            }
        };
        self.begin_pending(time)?;
        if observation.visible.is_none() {
            let blocks = self
                .producer
                .as_mut()
                .ok_or(CursorRuntimeError::Contract)?
                .gap(CursorTrajectoryGap {
                    segment_id: 0,
                    from_time_us: self.last_time,
                    to_time_us: None,
                    first_lost_observation_seq: Some(observation.sequence),
                    last_lost_observation_seq: Some(observation.sequence),
                    reason: CursorTrajectoryGapReason::SourceUnavailable,
                })
                .map_err(|_| CursorRuntimeError::Contract)?;
            self.emit(blocks)?;
            self.failed = true;
            return Err(CursorRuntimeError::Sink);
        }
        let expected = self.raw_sequence.map_or(0, |last| last.saturating_add(1));
        if observation.sequence < expected {
            self.failed = true;
            return Err(CursorRuntimeError::Clock);
        }
        if observation.sequence > expected {
            self.lost_input(expected, observation.sequence - 1, Some(time))?;
            self.begin_pending(time)?;
        }
        let position = self
            .context
            .frame
            .point(observation.x, observation.y)
            .and_then(|(x, y)| Some((u64::try_from(x).ok()? * 256, u64::try_from(y).ok()? * 256)));
        let sample = CursorTrajectorySample {
            segment_id: 0,
            observation_seq: observation.sequence,
            active_time_us: time,
            x_q8: position.map(|point| point.0),
            y_q8: position.map(|point| point.1),
            observed: position.is_some(),
            visible: observation.visible.unwrap_or(false),
            event: observation.event,
            button: observation.button,
            related_observation_seq: None,
            anchors: Vec::new(),
        };
        let blocks = self
            .producer
            .as_mut()
            .ok_or(CursorRuntimeError::Contract)?
            .observe(sample)
            .map_err(|_| CursorRuntimeError::Contract)?;
        self.raw_sequence = Some(observation.sequence);
        self.last_time = time;
        self.emit(blocks)
    }

    #[cfg(windows)]
    pub(super) fn tail_loss(&mut self, last_seen: Option<u64>) -> Result<(), CursorRuntimeError> {
        let Some(last) = last_seen else {
            return Ok(());
        };
        let first = self
            .raw_sequence
            .map_or(0, |sequence| sequence.saturating_add(1));
        if last < first {
            return Ok(());
        }
        if self.phase == Phase::Capturing && !self.failed && self.producer.is_some() {
            self.lost_input(first, last, None)?;
        }
        self.raw_sequence = Some(last);
        Ok(())
    }

    fn lost_input(
        &mut self,
        first: u64,
        last: u64,
        to: Option<u64>,
    ) -> Result<(), CursorRuntimeError> {
        // A second overflow while the earlier gap is open needs no invented segment.
        if self.pending_segment.is_none() {
            let blocks = self
                .producer
                .as_mut()
                .ok_or(CursorRuntimeError::Contract)?
                .gap(CursorTrajectoryGap {
                    segment_id: 0,
                    from_time_us: self.last_time,
                    to_time_us: to,
                    first_lost_observation_seq: Some(first),
                    last_lost_observation_seq: Some(last),
                    reason: CursorTrajectoryGapReason::InputOverflow,
                })
                .map_err(|_| CursorRuntimeError::Contract)?;
            self.emit(blocks)?;
        }
        self.pending_segment = Some(CursorTrajectorySegmentReason::AfterGap);
        Ok(())
    }

    fn begin_pending(&mut self, time: u64) -> Result<(), CursorRuntimeError> {
        let Some(reason) = self.pending_segment else {
            return Ok(());
        };
        let blocks = self
            .producer
            .as_mut()
            .ok_or(CursorRuntimeError::Contract)?
            .begin_segment(
                time,
                (self.context.frame.width, self.context.frame.height),
                WindowsTrajectoryClock::metadata(),
                reason,
            )
            .map_err(|_| CursorRuntimeError::Contract)?;
        self.pending_segment = None;
        self.emit(blocks)
    }

    pub(super) fn pause(&mut self, source_ns: u64) -> Result<(), CursorRuntimeError> {
        if self.producer.is_none() || self.failed {
            self.phase = Phase::Paused;
            return Ok(());
        }
        let time = self
            .active_clock
            .pause(source_ns)
            .map_err(|_| CursorRuntimeError::Clock)?;
        if self.pending_segment.is_none() {
            let blocks = self
                .producer
                .as_mut()
                .ok_or(CursorRuntimeError::Contract)?
                .boundary(time, CursorTrajectoryBoundaryReason::Pause)
                .map_err(|_| CursorRuntimeError::Contract)?;
            self.emit(blocks)?;
        }
        self.phase = Phase::Paused;
        self.last_time = time;
        Ok(())
    }

    pub(super) fn resume(&mut self, source_ns: u64) -> Result<(), CursorRuntimeError> {
        if self.producer.is_none() || self.failed {
            self.phase = Phase::Capturing;
            return Ok(());
        }
        let time = self
            .active_clock
            .resume(source_ns)
            .map_err(|_| CursorRuntimeError::Clock)?;
        self.phase = Phase::Capturing;
        self.pending_segment
            .get_or_insert(CursorTrajectorySegmentReason::Resume);
        self.begin_pending(time)?;
        self.last_time = time;
        Ok(())
    }

    /// Runtime removes hooks before this bounded writer-finalization wait.
    pub(super) fn finish(&mut self, source_ns: u64) {
        if self.phase == Phase::Closed {
            return;
        }
        if self.producer.is_some() && !self.failed {
            match self.active_clock.stop(source_ns) {
                Ok(time) => {
                    if self.phase == Phase::Capturing && self.pending_segment.is_none() {
                        let result = self
                            .producer
                            .as_mut()
                            .ok_or(CursorRuntimeError::Contract)
                            .and_then(|producer| {
                                producer
                                    .boundary(time, CursorTrajectoryBoundaryReason::Stop)
                                    .map_err(|_| CursorRuntimeError::Contract)
                            })
                            .and_then(|blocks| self.emit(blocks));
                        if result.is_err() {
                            self.failed = true;
                        }
                    }
                    if !self.failed {
                        let result = self
                            .producer
                            .as_mut()
                            .ok_or(CursorRuntimeError::Contract)
                            .and_then(|producer| {
                                producer.finish().map_err(|_| CursorRuntimeError::Contract)
                            })
                            .and_then(|blocks| self.emit(blocks));
                        if result.is_err() {
                            self.failed = true;
                        }
                    }
                }
                Err(_) => self.failed = true,
            }
        }
        if let Some(sink) = self.sink.as_mut() {
            self.descriptor = sink.finish(
                self.failed
                    || self
                        .producer
                        .as_ref()
                        .is_some_and(CursorTrajectoryProducer::incomplete),
            );
        }
        self.phase = Phase::Closed;
    }

    pub(super) fn erase(&mut self) -> Result<(), CursorRuntimeError> {
        self.phase = Phase::Closed;
        self.failed = true;
        if let Some(sink) = self.sink.as_mut() {
            sink.erase().map_err(|_| CursorRuntimeError::Sink)?;
        }
        Ok(())
    }

    pub(super) fn descriptor(&self) -> &CursorTrajectoryDescriptor {
        &self.descriptor
    }

    fn emit(&mut self, blocks: Vec<Vec<u8>>) -> Result<(), CursorRuntimeError> {
        for block in blocks {
            if self
                .sink
                .as_mut()
                .ok_or(CursorRuntimeError::Sink)?
                .try_append(block)
                .is_err()
            {
                self.failed = true;
                return Err(CursorRuntimeError::Sink);
            }
        }
        Ok(())
    }
}

fn unavailable(trajectory_id: String) -> CursorTrajectoryDescriptor {
    CursorTrajectoryDescriptor {
        format: CursorTrajectoryFormat::V1,
        format_version: 1,
        algorithm: CursorTrajectoryAlgorithm::SedWindowV1,
        encoding: CursorTrajectoryEncoding::JsonAsciiIntV1,
        trajectory_id,
        state: CursorTrajectoryState::Unavailable,
        finalized: true,
        total_blocks: 0,
        total_records: 0,
        total_samples: 0,
        total_bytes: 0,
        stream_sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            .to_owned(),
    }
}
