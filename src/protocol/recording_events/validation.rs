//! Source-byte admission; typed integers reject fractional/exponent/-0 tokens before rounding.

use super::{
    labels::valid_event_key_label,
    records::{
        EventAction, EventActionData, EventActionKind, EventBlock, EventCadence,
        EventCadenceEncoding, EventCompletion, EventKeyboardHoldData, EventPoint, EventPrivacy,
        EventRecord, EventScrollType, EventSignal, EventSignalData, EventSignalKind,
    },
};
use crate::protocol::trajectory::{MAX_SAFE_INTEGER, valid_trajectory_id};
use serde_json::Value;

use super::EventContractError;

pub(crate) fn decode_event_block(bytes: &[u8]) -> Result<EventBlock, EventContractError> {
    if bytes.len() > 65536 {
        return Err(EventContractError::Oversize);
    }
    let block: EventBlock =
        serde_json::from_slice(bytes).map_err(|_| EventContractError::Malformed)?;
    validate_event_block(&block)?;
    Ok(block)
}

pub(crate) fn validate_event_block(block: &EventBlock) -> Result<(), EventContractError> {
    if block.format_version != 1
        || !id(&block.recording_id)
        || !valid_trajectory_id(&block.event_stream_id)
        || block.records.is_empty()
        || block.records.len() > 512
    {
        return Err(EventContractError::Invalid);
    }
    let mut first = u64::MAX;
    let mut last = 0;
    let mut previous_seq = None;
    for (index, record) in block.records.iter().enumerate() {
        if matches!(
            record,
            EventRecord::Boundary { .. } | EventRecord::Gap { .. }
        ) && index + 1 != block.records.len()
        {
            return Err(EventContractError::Invalid);
        }
        let (seq, _, time) = record.position();
        if previous_seq.is_some_and(|previous: u64| previous.checked_add(1) != Some(seq)) {
            return Err(EventContractError::Invalid);
        }
        previous_seq = Some(seq);
        first = first.min(time);
        last = last.max(time);
        if let EventRecord::Gap {
            to_time_us: Some(end),
            ..
        } = record
        {
            last = last.max(*end);
        }
        validate_record(record)?;
    }
    if first != block.start_time_us || last != block.end_time_us {
        return Err(EventContractError::Invalid);
    }
    let value = serde_json::to_value(block).map_err(|_| EventContractError::Invalid)?;
    if !safe_integers(&value, "") {
        return Err(EventContractError::Invalid);
    }
    Ok(())
}

impl EventRecord {
    pub(crate) const fn position(&self) -> (u64, u64, u64) {
        match self {
            Self::Segment {
                record_seq,
                segment_id,
                active_time_us,
                ..
            }
            | Self::Boundary {
                record_seq,
                segment_id,
                active_time_us,
                ..
            }
            | Self::Action {
                record_seq,
                segment_id,
                active_time_us,
                ..
            }
            | Self::ActionUpdate {
                record_seq,
                segment_id,
                active_time_us,
                ..
            }
            | Self::Signal {
                record_seq,
                segment_id,
                active_time_us,
                ..
            }
            | Self::KeyboardRepeatBlock {
                record_seq,
                segment_id,
                active_time_us,
                ..
            }
            | Self::Gap {
                record_seq,
                segment_id,
                active_time_us,
                ..
            } => (*record_seq, *segment_id, *active_time_us),
        }
    }
}

fn validate_record(record: &EventRecord) -> Result<(), EventContractError> {
    let valid = match record {
        EventRecord::Segment { clock, .. } => {
            clock.timestamp_quantum_us.is_none_or(|value| value > 0)
        }
        EventRecord::Boundary { .. } => true,
        EventRecord::Action { action, .. } => action_valid(action),
        EventRecord::ActionUpdate {
            action_id,
            revision,
            data,
            ..
        } => {
            use super::records::EventActionUpdate;
            id(action_id)
                && *revision > 0
                && match data {
                    EventActionUpdate::Key(data) => data.completion != EventCompletion::Semantic,
                    EventActionUpdate::Click(data) => point_valid(data.up_point.as_ref()),
                }
        }
        EventRecord::Signal { signal, .. } => signal_valid(signal),
        EventRecord::KeyboardRepeatBlock {
            owner_id, cadence, ..
        } => id(owner_id) && cadence_valid(cadence),
        EventRecord::Gap {
            from_time_us,
            to_time_us,
            first_lost_source_seq,
            last_lost_source_seq,
            lost_count,
            ..
        } => {
            to_time_us.is_none_or(|end| end >= *from_time_us)
                && match (first_lost_source_seq, last_lost_source_seq, lost_count) {
                    (None, None, None) => true,
                    (Some(first), Some(last), Some(count)) => {
                        last.checked_sub(*first)
                            .and_then(|delta| delta.checked_add(1))
                            == Some(*count)
                    }
                    _ => false,
                }
        }
    };
    if valid {
        Ok(())
    } else {
        Err(EventContractError::Invalid)
    }
}

fn action_valid(action: &EventAction) -> bool {
    if !id(&action.id) || !point_valid(action.point.as_ref()) {
        return false;
    }
    match (&action.kind, &action.data) {
        (EventActionKind::Key, EventActionData::Key(data)) => {
            valid_event_key_label(&action.label)
                && action.point.is_none()
                && data.completion != EventCompletion::Semantic
                && optional_id(data.keyboard_attack_id.as_deref())
                && optional_id(data.group_id.as_deref())
                && optional_id(data.continuation_of.as_deref())
        }
        (EventActionKind::Click, EventActionData::Click(data)) => {
            action.label == "Click"
                && data.button <= 31
                && data.click_count > 0
                && optional_id(data.gesture_id.as_deref())
                && point_valid(data.up_point.as_ref())
        }
        (EventActionKind::Scroll, EventActionData::Scroll(data)) => {
            action.label == "Scroll"
                && data.delta_mode.is_none_or(|mode| mode <= 2)
                && [
                    data.delta_x,
                    data.delta_y,
                    data.delta_z,
                    data.scroll_x,
                    data.scroll_y,
                ]
                .into_iter()
                .flatten()
                .all(f64::is_finite)
                && match data.event_type {
                    Some(EventScrollType::Wheel) => {
                        data.delta_x.is_some()
                            && data.delta_y.is_some()
                            && data.delta_z.is_some()
                            && data.delta_mode.is_some()
                    }
                    Some(EventScrollType::Scroll) => {
                        data.scroll_x.is_some() && data.scroll_y.is_some()
                    }
                    None => true,
                }
        }
        (EventActionKind::Pause, EventActionData::Pause(_)) => action.label == "Pause",
        _ => false,
    }
}

fn signal_valid(signal: &EventSignal) -> bool {
    if !id(&signal.id)
        || signal.start_time_us > signal.end_time_us
        || !point_valid(signal.point.as_ref())
    {
        return false;
    }
    match (&signal.kind, &signal.data) {
        (EventSignalKind::Typing, EventSignalData::Typing(data)) => {
            signal.point.is_none()
                && optional_id(data.group_id.as_deref())
                && optional_id(data.continuation_of.as_deref())
                && match data.privacy {
                    EventPrivacy::Protected => {
                        data.event_count.is_none()
                            && data.group_id.is_none()
                            && data.continuation_of.is_none()
                            && signal.start_time_us.is_multiple_of(1_000_000)
                            && signal.end_time_us.is_multiple_of(1_000_000)
                    }
                    EventPrivacy::Normal | EventPrivacy::Unknown => {
                        data.event_count.is_some_and(|count| count > 0)
                    }
                }
        }
        (EventSignalKind::KeyboardHold, EventSignalData::KeyboardHold(data)) => {
            keyboard_hold_valid(signal, data)
        }
        (EventSignalKind::CursorIdle, EventSignalData::CursorIdle(_)) => true,
        (EventSignalKind::StaticFrame, EventSignalData::Warning(data)) => {
            signal.point.is_none() && warning_valid(data.source.as_deref(), &data.code)
        }
        _ => false,
    }
}

fn keyboard_hold_valid(signal: &EventSignal, data: &EventKeyboardHoldData) -> bool {
    signal.point.is_none()
        && data.privacy == EventPrivacy::Normal
        && matches!(
            data.completion,
            EventCompletion::Released | EventCompletion::Lost | EventCompletion::Cancelled
        )
        && data
            .down_time_us
            .is_none_or(|time| time == signal.start_time_us)
        && data
            .up_time_us
            .is_none_or(|time| time == signal.end_time_us)
        && data
            .last_observed_time_us
            .is_none_or(|time| time == signal.end_time_us)
        && data.press_duration_us.is_none_or(|duration| {
            data.down_time_us
                .zip(data.up_time_us)
                .and_then(|(down, up)| up.checked_sub(down))
                == Some(duration)
        })
}

fn cadence_valid(cadence: &EventCadence) -> bool {
    if cadence.version != 1
        || !id(&cadence.id)
        || cadence.quantum_us != 1
        || cadence.count == 0
        || cadence.count > 512
        || cadence.ticks.first() != Some(&0)
        || cadence.ticks.len() > 1024
    {
        return false;
    }
    match cadence.encoding {
        EventCadenceEncoding::DeltaUs => cadence.ticks.len() == usize::from(cadence.count),
        EventCadenceEncoding::RleUs => {
            cadence.ticks.len().is_multiple_of(2)
                && cadence
                    .ticks
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .all(|pair| pair[1] > 0)
                && cadence
                    .ticks
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .try_fold(0_u64, |sum, pair| sum.checked_add(pair[1]))
                    == Some(u64::from(cadence.count))
        }
    }
}

fn safe_integers(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(object) => object.iter().all(|(key, value)| safe_integers(value, key)),
        Value::Array(items) => items.iter().all(|value| safe_integers(value, key)),
        Value::Number(number)
            if matches!(
                key,
                "x" | "y" | "deltaX" | "deltaY" | "deltaZ" | "scrollX" | "scrollY"
            ) =>
        {
            number.as_f64().is_some_and(f64::is_finite)
        }
        Value::Number(number) => number
            .as_u64()
            .is_some_and(|value| value <= MAX_SAFE_INTEGER),
        _ => true,
    }
}

fn point_valid(point: Option<&EventPoint>) -> bool {
    point.is_none_or(|point| (0.0..=1.0).contains(&point.x) && (0.0..=1.0).contains(&point.y))
}
fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}
fn optional_id(value: Option<&str>) -> bool {
    value.is_none_or(id)
}

fn warning_valid(source: Option<&str>, code: &str) -> bool {
    match source {
        Some("keyboard-hook" | "mouse-hook") => matches!(
            code,
            "events-dropped"
                | "drain-failed"
                | "signal-record-failed"
                | "event-record-failed"
                | "blocked"
                | "permission-denied"
                | "policy-denied"
                | "install-failed"
        ),
        Some("cursor-telemetry") => code == "idle-signal-record-failed",
        Some("cursor-trajectory") => code == "format-unavailable",
        Some("recording-events") => matches!(
            code,
            "format-unavailable"
                | "input-overflow"
                | "writer-backpressure"
                | "storage-failure"
                | "source-unavailable"
                | "clock-discontinuity"
                | "unknown-record"
                | "oversize-record"
                | "counter-overflow"
        ),
        None => code == "source-unavailable",
        _ => false,
    }
}

#[cfg(test)]
pub(crate) fn recording_events_fixture() -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_slice(include_bytes!(
        "../../../contracts/native/fixtures/recording-events-v1.json"
    ))
}
