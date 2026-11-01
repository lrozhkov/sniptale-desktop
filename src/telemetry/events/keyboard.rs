//! Bounded active command attacks and anonymous aggregate continuations; no retained history list.

use super::emitter::EventEmitter;
use crate::{
    protocol::recording_events::{
        EventAction, EventActionData, EventActionKind, EventActionUpdate, EventCadence,
        EventCadenceEncoding, EventCadencePhase, EventCompletion, EventKeyData, EventKeyUpdate,
        EventOwnerKind, EventPreset, EventPrivacy, EventRecord, EventSignal, EventSignalData,
        EventSignalKind, EventTypingActivity, EventTypingData,
    },
    telemetry::{
        CursorTrajectorySinkError,
        sanitize::SanitizedKeySignal,
        windows::{WindowsKeyEvent, WindowsKeyPhase, sanitize_windows_key_event},
    },
};

#[derive(Debug)]
struct Command {
    id: String,
    down: Option<u64>,
    last: u64,
    repeats: u64,
    cadence: Vec<u64>,
}

#[derive(Debug)]
struct Anonymous {
    first: u64,
    last: u64,
    count: u16,
    group: String,
    continuation: Option<String>,
}

#[derive(Debug)]
pub(super) struct KeyboardHistory {
    keys: [Option<Command>; 256],
    typing: Option<Anonymous>,
    collect_keys: bool,
    collect_typing: bool,
}

impl Default for KeyboardHistory {
    fn default() -> Self {
        Self {
            keys: std::array::from_fn(|_| None),
            typing: None,
            collect_keys: false,
            collect_typing: false,
        }
    }
}

impl KeyboardHistory {
    pub(super) fn new(collect_keys: bool, collect_typing: bool) -> Self {
        Self {
            collect_keys,
            collect_typing,
            ..Self::default()
        }
    }

    pub(super) fn missing_onset_context(
        &self,
        event: WindowsKeyEvent,
        phase: WindowsKeyPhase,
    ) -> bool {
        let Some(command) = usize::try_from(event.virtual_key)
            .ok()
            .and_then(|key| self.keys.get(key))
        else {
            return false;
        };
        self.collect_keys
            && phase != WindowsKeyPhase::Down
            && command.is_none()
            && matches!(
                sanitize_windows_key_event(event),
                SanitizedKeySignal::KeyLabel(_) | SanitizedKeySignal::Shortcut(_)
            )
    }

    pub(super) fn observe(
        &mut self,
        event: WindowsKeyEvent,
        phase: WindowsKeyPhase,
        time: u64,
        output: &mut EventEmitter,
    ) -> Result<(), CursorTrajectorySinkError> {
        let index =
            usize::try_from(event.virtual_key).map_err(|_| CursorTrajectorySinkError::Storage)?;
        if index >= 256 {
            return Err(CursorTrajectorySinkError::Storage);
        }
        if phase == WindowsKeyPhase::Up {
            if let Some(command) = self.keys[index].take() {
                close(command, Some(time), time, output)?;
            } else if self.collect_keys {
                match sanitize_windows_key_event(event) {
                    SanitizedKeySignal::KeyLabel(label) | SanitizedKeySignal::Shortcut(label) => {
                        release_only(event, label, time, output)?;
                    }
                    _ => {}
                }
            }
            return Ok(());
        }
        let label = match sanitize_windows_key_event(event) {
            SanitizedKeySignal::KeyLabel(label) | SanitizedKeySignal::Shortcut(label) => label,
            SanitizedKeySignal::Typing { count } => {
                if let Some(command) = self.keys[index].take() {
                    close(command, None, time, output)?;
                }
                return if self.collect_typing {
                    self.anonymous(time, count, output)
                } else {
                    Ok(())
                };
            }
            SanitizedKeySignal::Ignored => return Ok(()),
        };
        if !self.collect_keys {
            return Ok(());
        }
        self.flush_typing(output)?;
        let mut command = self.keys[index].take();
        if phase == WindowsKeyPhase::Down
            && let Some(previous) = command.take()
        {
            close(previous, None, time, output)?;
        }
        let mut command = match command {
            Some(command) => command,
            None => start(event, phase, label, time, output)?,
        };
        command.last = time;
        if phase == WindowsKeyPhase::Repeat {
            command.repeats = command
                .repeats
                .checked_add(1)
                .ok_or(CursorTrajectorySinkError::Storage)?;
            command.cadence.push(time);
            if command.cadence.len() == 512 {
                flush_repeats(&mut command, output)?;
            }
        }
        self.keys[index] = Some(command);
        Ok(())
    }

    fn anonymous(
        &mut self,
        time: u64,
        count: u16,
        output: &mut EventEmitter,
    ) -> Result<(), CursorTrajectorySinkError> {
        if count == 0 {
            return Ok(());
        }
        let overflow = self
            .typing
            .as_ref()
            .is_some_and(|span| span.count.checked_add(count).is_none());
        let continuation = if overflow {
            let span = self
                .typing
                .take()
                .ok_or(CursorTrajectorySinkError::Storage)?;
            let group = span.group.clone();
            Some((group, emit_typing(span, output)?))
        } else {
            None
        };
        if let Some(span) = self.typing.as_mut() {
            span.count = span
                .count
                .checked_add(count)
                .ok_or(CursorTrajectorySinkError::Storage)?;
            span.last = time;
        } else {
            let (group, previous) = continuation.map_or_else(
                || (output.id("typing-group"), None),
                |(group, previous)| (group, Some(previous)),
            );
            self.typing = Some(Anonymous {
                first: time,
                last: time,
                count,
                group,
                continuation: previous,
            });
        }
        Ok(())
    }

    pub(super) fn flush_typing(
        &mut self,
        output: &mut EventEmitter,
    ) -> Result<(), CursorTrajectorySinkError> {
        if let Some(span) = self.typing.take() {
            emit_typing(span, output)?;
        }
        Ok(())
    }

    pub(super) fn boundary(
        &mut self,
        time: u64,
        output: &mut EventEmitter,
    ) -> Result<(), CursorTrajectorySinkError> {
        self.flush_typing(output)?;
        for command in &mut self.keys {
            if let Some(command) = command.take() {
                close(command, None, time, output)?;
            }
        }
        Ok(())
    }
}

fn start(
    event: WindowsKeyEvent,
    phase: WindowsKeyPhase,
    label: String,
    time: u64,
    output: &mut EventEmitter,
) -> Result<Command, CursorTrajectorySinkError> {
    let id = output.id("key");
    let onset = (phase == WindowsKeyPhase::Down).then_some(time);
    let (ctrl, alt, shift, meta) = event.modifiers.facts();
    let action = EventAction {
        id: id.clone(),
        kind: EventActionKind::Key,
        time_us: time,
        duration_us: 0,
        point: None,
        label,
        preset: EventPreset::None,
        data: EventActionData::Key(EventKeyData {
            down_time_us: onset,
            up_time_us: None,
            press_duration_us: None,
            last_observed_time_us: time,
            repeat_count: 0,
            completion: EventCompletion::Observed,
            keyboard_attack_id: onset.map(|_| id.clone()),
            keyboard_onset_time_us: onset,
            group_id: Some(id.clone()),
            continuation_of: None,
            ctrl_key: ctrl,
            alt_key: alt,
            shift_key: shift,
            meta_key: meta,
        }),
    };
    output.emit(|record_seq, segment_id| EventRecord::Action {
        record_seq,
        segment_id,
        active_time_us: time,
        action,
    })?;
    if onset.is_some() {
        cadence(&id, &[time], EventCadencePhase::Onset, output)?;
    }
    Ok(Command {
        id,
        down: onset,
        last: time,
        repeats: 0,
        cadence: Vec::new(),
    })
}

fn release_only(
    event: WindowsKeyEvent,
    label: String,
    time: u64,
    output: &mut EventEmitter,
) -> Result<(), CursorTrajectorySinkError> {
    let (ctrl, alt, shift, meta) = event.modifiers.facts();
    let action = EventAction {
        id: output.id("key-release"),
        kind: EventActionKind::Key,
        time_us: time,
        duration_us: 0,
        point: None,
        label,
        preset: EventPreset::None,
        data: EventActionData::Key(EventKeyData {
            down_time_us: None,
            up_time_us: Some(time),
            press_duration_us: None,
            last_observed_time_us: time,
            repeat_count: 0,
            completion: EventCompletion::Released,
            keyboard_attack_id: None,
            keyboard_onset_time_us: None,
            group_id: None,
            continuation_of: None,
            ctrl_key: ctrl,
            alt_key: alt,
            shift_key: shift,
            meta_key: meta,
        }),
    };
    output.emit(|record_seq, segment_id| EventRecord::Action {
        record_seq,
        segment_id,
        active_time_us: time,
        action,
    })
}

fn cadence(
    owner: &str,
    times: &[u64],
    phase: EventCadencePhase,
    output: &mut EventEmitter,
) -> Result<(), CursorTrajectorySinkError> {
    let Some(first) = times.first().copied() else {
        return Ok(());
    };
    let mut previous = first;
    let mut ticks = Vec::with_capacity(times.len());
    for time in times {
        ticks.push(
            time.checked_sub(previous)
                .ok_or(CursorTrajectorySinkError::Storage)?,
        );
        previous = *time;
    }
    let cadence = EventCadence {
        version: 1,
        id: output.id("cadence"),
        start_time_us: first,
        quantum_us: 1,
        count: u16::try_from(times.len()).map_err(|_| CursorTrajectorySinkError::Storage)?,
        encoding: EventCadenceEncoding::DeltaUs,
        ticks,
    };
    output.emit(|record_seq, segment_id| EventRecord::KeyboardRepeatBlock {
        record_seq,
        segment_id,
        active_time_us: previous,
        owner_kind: EventOwnerKind::Action,
        owner_id: owner.to_owned(),
        phase,
        cadence,
    })
}

fn flush_repeats(
    command: &mut Command,
    output: &mut EventEmitter,
) -> Result<(), CursorTrajectorySinkError> {
    cadence(
        &command.id,
        &command.cadence,
        EventCadencePhase::Repeat,
        output,
    )?;
    command.cadence.clear();
    Ok(())
}

fn close(
    mut command: Command,
    up: Option<u64>,
    boundary: u64,
    output: &mut EventEmitter,
) -> Result<(), CursorTrajectorySinkError> {
    flush_repeats(&mut command, output)?;
    let last = up.unwrap_or(command.last);
    let duration = command
        .down
        .zip(up)
        .and_then(|(down, up)| up.checked_sub(down));
    let data = EventActionUpdate::Key(EventKeyUpdate {
        up_time_us: up,
        press_duration_us: duration,
        last_observed_time_us: last,
        repeat_count: command.repeats,
        completion: if up.is_some() {
            EventCompletion::Released
        } else {
            EventCompletion::Lost
        },
    });
    output.emit(|record_seq, segment_id| EventRecord::ActionUpdate {
        record_seq,
        segment_id,
        active_time_us: boundary,
        action_id: command.id,
        revision: 1,
        data,
    })
}

fn emit_typing(
    span: Anonymous,
    output: &mut EventEmitter,
) -> Result<String, CursorTrajectorySinkError> {
    let id = output.id("typing");
    let signal = EventSignal {
        id: id.clone(),
        kind: EventSignalKind::Typing,
        start_time_us: span.first,
        end_time_us: span.last,
        point: None,
        data: EventSignalData::Typing(EventTypingData {
            event_count: Some(u64::from(span.count)),
            activity: EventTypingActivity::Keyboard,
            privacy: EventPrivacy::Unknown,
            group_id: Some(span.group),
            continuation_of: span.continuation,
        }),
    };
    output.emit(|record_seq, segment_id| EventRecord::Signal {
        record_seq,
        segment_id,
        active_time_us: span.last,
        signal,
    })?;
    Ok(id)
}
