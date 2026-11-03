//! Recording-only event production with bounded active state and independently durable blocks.

mod emitter;
mod keyboard;
mod pointer;

use super::{
    CursorTrajectorySinkError, RecordingEventsSinkFactory, RecordingTelemetryPolicy,
    RecordingTrajectoryContext,
    events_clock::EventActiveClock,
    trajectory_clock::{TrajectoryClockError, WindowsTrajectoryClock},
    windows::WindowsKeyObservation,
};
use crate::protocol::recording_events::{
    EventBoundaryReason, EventClock, EventDescriptor, EventGapReason, EventGapSource, EventRecord,
    EventSegmentReason, EventSeverity, EventSignal, EventSignalData, EventSignalKind,
    EventWarningData,
};
use emitter::EventEmitter;
use keyboard::KeyboardHistory;
use pointer::PointerHistory;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Phase {
    Capturing,
    Paused,
    Stopped,
}

#[derive(Debug)]
pub(crate) struct NativeRecordingEvents {
    emitter: EventEmitter,
    keyboard: KeyboardHistory,
    pointer: PointerHistory,
    collect_pointer: bool,
    last_mouse_sequence: Option<u64>,
    clock: EventActiveClock,
    source_clock: Option<WindowsTrajectoryClock>,
    last_keyboard_sequence: Option<u64>,
    keyboard_context_reported: bool,
    phase: Phase,
}

impl NativeRecordingEvents {
    pub(crate) fn new(
        recording: &str,
        source: String,
        context: &RecordingTrajectoryContext,
        policy: RecordingTelemetryPolicy,
        started: u64,
        clock: Option<WindowsTrajectoryClock>,
        factory: Option<&mut dyn RecordingEventsSinkFactory>,
    ) -> Self {
        let sink = clock.and_then(|_| {
            factory.and_then(|factory| factory.create(recording, &source, context).ok())
        });
        let mut state = Self {
            emitter: EventEmitter::new(recording.to_owned(), source, sink),
            keyboard: KeyboardHistory::new(policy.collect_key_events, policy.collect_typing_spans),
            pointer: PointerHistory::new(
                (context.point_mapping == super::RecordingPointMapping::FixedDesktopFrame)
                    .then_some(context.frame),
            ),
            collect_pointer: policy.collect_clicks,
            last_mouse_sequence: None,
            clock: EventActiveClock::new(started),
            source_clock: clock,
            last_keyboard_sequence: None,
            keyboard_context_reported: false,
            phase: Phase::Capturing,
        };
        if clock.is_none() {
            state.emitter.failed = true;
        }
        if state.segment(0, EventSegmentReason::Start).is_err() {
            state.emitter.failed = true;
        }
        state
    }

    pub(crate) fn descriptor(&self) -> &EventDescriptor {
        self.emitter.descriptor()
    }
    pub(crate) fn fail(&mut self) {
        self.emitter.failed = true;
    }

    pub(crate) fn keyboard(
        &mut self,
        observation: WindowsKeyObservation,
    ) -> Result<(), CursorTrajectorySinkError> {
        if self.emitter.failed {
            return Err(CursorTrajectorySinkError::Retired);
        }
        let source = self
            .source_clock
            .ok_or(CursorTrajectorySinkError::Storage)?
            .event_monotonic_ns(observation.source_time_ms)
            .map_err(|_| CursorTrajectorySinkError::Storage)?;
        let time = match self.clock.observe(source) {
            Ok(time) => time,
            Err(TrajectoryClockError::BeforeStart) => {
                self.last_keyboard_sequence = Some(observation.sequence);
                return Ok(());
            }
            Err(_) => return Err(CursorTrajectorySinkError::Storage),
        };
        let expected = self
            .last_keyboard_sequence
            .map_or(Some(0), |previous| previous.checked_add(1))
            .ok_or(CursorTrajectorySinkError::Storage)?;
        if observation.sequence < expected {
            return Err(CursorTrajectorySinkError::Storage);
        }
        if observation.sequence > expected {
            self.gap(
                time,
                EventGapSource::Keyboard,
                Some((expected, observation.sequence - 1)),
                EventGapReason::InputOverflow,
            )?;
        }
        self.last_keyboard_sequence = Some(observation.sequence);
        if !self.keyboard_context_reported
            && self
                .keyboard
                .missing_onset_context(observation.event, observation.phase)
        {
            self.gap_record(
                time,
                EventGapSource::Keyboard,
                None,
                EventGapReason::SourceUnavailable,
            )?;
            self.keyboard_context_reported = true;
        }
        self.keyboard.observe(
            observation.event,
            observation.phase,
            time,
            &mut self.emitter,
        )
    }

    pub(crate) fn mouse(
        &mut self,
        event: super::windows::WindowsMouseEvent,
    ) -> Result<(), CursorTrajectorySinkError> {
        if !self.collect_pointer {
            return Ok(());
        }
        if self.emitter.failed {
            return Err(CursorTrajectorySinkError::Retired);
        }
        let source = self
            .source_clock
            .ok_or(CursorTrajectorySinkError::Storage)?
            .event_monotonic_ns(event.source_time_ms)
            .map_err(|_| CursorTrajectorySinkError::Storage)?;
        let time = match self.clock.observe(source) {
            Ok(time) => time,
            Err(TrajectoryClockError::BeforeStart) => {
                self.last_mouse_sequence = Some(event.sequence);
                return Ok(());
            }
            Err(_) => return Err(CursorTrajectorySinkError::Storage),
        };
        let expected = self
            .last_mouse_sequence
            .map_or(Some(0), |previous| previous.checked_add(1))
            .ok_or(CursorTrajectorySinkError::Storage)?;
        if event.sequence < expected {
            return Err(CursorTrajectorySinkError::Storage);
        }
        if event.sequence > expected {
            self.gap(
                time,
                EventGapSource::Producer,
                Some((expected, event.sequence - 1)),
                EventGapReason::InputOverflow,
            )?;
        }
        self.last_mouse_sequence = Some(event.sequence);
        self.pointer.observe(event, time, &mut self.emitter)
    }

    #[cfg(windows)]
    pub(crate) fn paused_mouse_baseline(&mut self, sequence: u64) {
        self.last_mouse_sequence = Some(sequence);
    }

    #[cfg(windows)]
    pub(crate) fn paused_keyboard_baseline(&mut self, sequence: u64) {
        self.last_keyboard_sequence = Some(sequence);
    }

    pub(crate) fn warning(
        &mut self,
        source: &'static str,
        code: &'static str,
        monotonic: u64,
    ) -> Result<(), CursorTrajectorySinkError> {
        if self.phase != Phase::Capturing {
            return Ok(());
        }
        let time = self
            .clock
            .observe(monotonic)
            .map_err(|_| CursorTrajectorySinkError::Storage)?;
        let signal = EventSignal {
            id: self.emitter.id("warning"),
            kind: EventSignalKind::StaticFrame,
            start_time_us: time,
            end_time_us: time,
            point: None,
            data: EventSignalData::Warning(EventWarningData {
                source: Some(source.to_owned()),
                code: code.to_owned(),
                severity: EventSeverity::Warning,
            }),
        };
        self.emitter
            .emit(|record_seq, segment_id| EventRecord::Signal {
                record_seq,
                segment_id,
                active_time_us: time,
                signal,
            })?;
        let lost = source == "keyboard-hook"
            || source == "mouse-hook" && self.collect_pointer
            || source == "cursor-telemetry";
        if lost {
            let origin = if source == "keyboard-hook" {
                EventGapSource::Keyboard
            } else {
                EventGapSource::Producer
            };
            let reason = if code == "events-dropped" {
                EventGapReason::InputOverflow
            } else {
                EventGapReason::SourceUnavailable
            };
            self.gap(time, origin, None, reason)?;
        }
        Ok(())
    }

    pub(crate) fn pause(&mut self, monotonic: u64) -> Result<(), CursorTrajectorySinkError> {
        if self.phase != Phase::Capturing {
            return Err(CursorTrajectorySinkError::Retired);
        }
        let time = self
            .clock
            .pause(monotonic)
            .map_err(|_| CursorTrajectorySinkError::Storage)?;
        self.keyboard.boundary(time, &mut self.emitter)?;
        self.pointer.boundary(time, &mut self.emitter)?;
        self.boundary(time, EventBoundaryReason::Pause)?;
        self.phase = Phase::Paused;
        Ok(())
    }

    pub(crate) fn resume(&mut self, monotonic: u64) -> Result<(), CursorTrajectorySinkError> {
        if self.phase != Phase::Paused {
            return Err(CursorTrajectorySinkError::Retired);
        }
        let time = self
            .clock
            .resume(monotonic)
            .map_err(|_| CursorTrajectorySinkError::Storage)?;
        self.next_segment(time, EventSegmentReason::Resume)?;
        self.phase = Phase::Capturing;
        Ok(())
    }

    pub(crate) fn finish(&mut self, monotonic: u64) -> EventDescriptor {
        if self.phase == Phase::Stopped {
            return self.emitter.descriptor().clone();
        }
        let result = self
            .clock
            .stop(monotonic)
            .map_err(|_| CursorTrajectorySinkError::Storage)
            .and_then(|time| {
                if self.phase == Phase::Capturing {
                    self.keyboard.boundary(time, &mut self.emitter)?;
                    self.pointer.boundary(time, &mut self.emitter)?;
                    self.boundary(time, EventBoundaryReason::Stop)?;
                }
                Ok(())
            });
        if result.is_err() {
            self.emitter.failed = true;
        }
        self.phase = Phase::Stopped;
        self.emitter.finish()
    }

    pub(crate) fn erase(&mut self) -> Result<(), CursorTrajectorySinkError> {
        self.emitter.erase()
    }

    fn gap(
        &mut self,
        time: u64,
        source: EventGapSource,
        bounds: Option<(u64, u64)>,
        reason: EventGapReason,
    ) -> Result<(), CursorTrajectorySinkError> {
        self.keyboard.boundary(time, &mut self.emitter)?;
        self.pointer.boundary(time, &mut self.emitter)?;
        self.gap_record(time, source, bounds, reason)
    }

    // Unknown onset context is not evidence that independently observed input pairs were lost.
    fn gap_record(
        &mut self,
        time: u64,
        source: EventGapSource,
        bounds: Option<(u64, u64)>,
        reason: EventGapReason,
    ) -> Result<(), CursorTrajectorySinkError> {
        let count = bounds.and_then(|(first, last)| {
            last.checked_sub(first)
                .and_then(|delta| delta.checked_add(1))
        });
        self.emitter
            .emit(|record_seq, segment_id| EventRecord::Gap {
                record_seq,
                segment_id,
                active_time_us: time,
                source,
                from_time_us: time,
                to_time_us: Some(time),
                first_lost_source_seq: bounds.map(|bound| bound.0),
                last_lost_source_seq: bounds.map(|bound| bound.1),
                lost_count: count,
                reason,
            })?;
        self.next_segment(time, EventSegmentReason::AfterGap)
    }

    fn boundary(
        &mut self,
        time: u64,
        reason: EventBoundaryReason,
    ) -> Result<(), CursorTrajectorySinkError> {
        self.emitter
            .emit(|record_seq, segment_id| EventRecord::Boundary {
                record_seq,
                segment_id,
                active_time_us: time,
                reason,
            })
    }

    fn next_segment(
        &mut self,
        time: u64,
        reason: EventSegmentReason,
    ) -> Result<(), CursorTrajectorySinkError> {
        self.emitter.segment = self
            .emitter
            .segment
            .checked_add(1)
            .ok_or(CursorTrajectorySinkError::Storage)?;
        self.segment(time, reason)
    }

    fn segment(
        &mut self,
        time: u64,
        reason: EventSegmentReason,
    ) -> Result<(), CursorTrajectorySinkError> {
        let clock = WindowsTrajectoryClock::metadata();
        self.emitter
            .emit(|record_seq, segment_id| EventRecord::Segment {
                record_seq,
                segment_id,
                active_time_us: time,
                reason,
                clock: EventClock {
                    source: clock.source,
                    timestamp_quantum_us: clock.timestamp_quantum_us,
                    resolution_bound_us: clock.resolution_bound_us,
                    mapping_uncertainty_us: clock.mapping_uncertainty_us,
                },
            })
    }
}
