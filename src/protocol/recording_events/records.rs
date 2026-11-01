//! Required-field DTOs for sanitized immutable event blocks.

use crate::protocol::trajectory::CursorTrajectoryClockSource;
use serde::{Deserialize, Deserializer, Serialize};

fn nullable<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventBlock {
    pub(crate) format_version: u16,
    pub(crate) recording_id: String,
    pub(crate) event_stream_id: String,
    pub(crate) sequence: u64,
    pub(crate) start_time_us: u64,
    pub(crate) end_time_us: u64,
    pub(crate) records: Vec<EventRecord>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum EventRecord {
    Segment {
        record_seq: u64,
        segment_id: u64,
        active_time_us: u64,
        clock: EventClock,
        reason: EventSegmentReason,
    },
    Boundary {
        record_seq: u64,
        segment_id: u64,
        active_time_us: u64,
        reason: EventBoundaryReason,
    },
    Action {
        record_seq: u64,
        segment_id: u64,
        active_time_us: u64,
        action: EventAction,
    },
    ActionUpdate {
        record_seq: u64,
        segment_id: u64,
        active_time_us: u64,
        action_id: String,
        revision: u64,
        data: EventActionUpdate,
    },
    Signal {
        record_seq: u64,
        segment_id: u64,
        active_time_us: u64,
        signal: EventSignal,
    },
    KeyboardRepeatBlock {
        record_seq: u64,
        segment_id: u64,
        active_time_us: u64,
        owner_kind: EventOwnerKind,
        owner_id: String,
        phase: EventCadencePhase,
        cadence: EventCadence,
    },
    Gap {
        record_seq: u64,
        segment_id: u64,
        active_time_us: u64,
        source: EventGapSource,
        from_time_us: u64,
        #[serde(deserialize_with = "nullable")]
        to_time_us: Option<u64>,
        #[serde(deserialize_with = "nullable")]
        first_lost_source_seq: Option<u64>,
        #[serde(deserialize_with = "nullable")]
        last_lost_source_seq: Option<u64>,
        #[serde(deserialize_with = "nullable")]
        lost_count: Option<u64>,
        reason: EventGapReason,
    },
}

macro_rules! closed_enum {
    ($name:ident { $($variant:ident => $wire:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
        pub(crate) enum $name { $(#[serde(rename = $wire)] $variant),+ }
    };
}

closed_enum!(EventSegmentReason { Start => "start", Resume => "resume", Visibility => "visibility",
    Privacy => "privacy", ClockDiscontinuity => "clock-discontinuity", AfterGap => "after-gap" });
closed_enum!(EventBoundaryReason { Pause => "pause", Stop => "stop", Visibility => "visibility",
    Privacy => "privacy", ClockDiscontinuity => "clock-discontinuity" });
closed_enum!(EventGapSource { Keyboard => "keyboard", Signal => "signal", Producer => "producer" });
closed_enum!(EventGapReason { InputOverflow => "input-overflow", WriterBackpressure => "writer-backpressure",
    StorageFailure => "storage-failure", SourceUnavailable => "source-unavailable",
    ClockDiscontinuity => "clock-discontinuity", UnknownRecord => "unknown-record",
    OversizeRecord => "oversize-record", CounterOverflow => "counter-overflow" });
closed_enum!(EventOwnerKind { Action => "action", Signal => "signal" });
closed_enum!(EventCadencePhase { Onset => "onset", Repeat => "repeat" });
closed_enum!(EventCadenceEncoding { DeltaUs => "delta-us", RleUs => "rle-us" });
closed_enum!(EventPrivacy { Normal => "normal", Protected => "protected", Unknown => "unknown" });
closed_enum!(EventTypingActivity { Keyboard => "keyboard", Input => "input" });
closed_enum!(EventCompletion { Observed => "observed", Released => "released", Lost => "lost",
    Cancelled => "cancelled", Semantic => "semantic" });
closed_enum!(EventGesture { Click => "click", Hold => "hold", Drag => "drag", Selection => "selection",
    NativeDrag => "native-drag" });
closed_enum!(EventScrollType { Wheel => "wheel", Scroll => "scroll" });
closed_enum!(EventPausePhase { Pause => "pause", Resume => "resume" });
closed_enum!(EventActionKind { Key => "KEY", Click => "CLICK", Scroll => "SCROLL", Pause => "PAUSE" });
closed_enum!(EventPreset { None => "NONE" });
closed_enum!(EventSignalKind { Typing => "typing", KeyboardHold => "keyboard-hold",
    CursorIdle => "cursor-idle", StaticFrame => "static-frame" });
closed_enum!(EventCursorSource { NativeCursor => "native-cursor", BrowserCursor => "browser-cursor" });
closed_enum!(EventSeverity { Warning => "warning" });

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventPoint {
    pub(crate) x: f64,
    pub(crate) y: f64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventCadence {
    pub(crate) version: u16,
    pub(crate) id: String,
    pub(crate) start_time_us: u64,
    pub(crate) quantum_us: u64,
    pub(crate) count: u16,
    pub(crate) encoding: EventCadenceEncoding,
    pub(crate) ticks: Vec<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventAction {
    pub(crate) id: String,
    pub(crate) kind: EventActionKind,
    pub(crate) time_us: u64,
    pub(crate) duration_us: u64,
    #[serde(deserialize_with = "nullable")]
    pub(crate) point: Option<EventPoint>,
    pub(crate) label: String,
    pub(crate) data: EventActionData,
    pub(crate) preset: EventPreset,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub(crate) enum EventActionData {
    Key(EventKeyData),
    Click(EventClickData),
    Scroll(EventScrollData),
    Pause(EventPauseData),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventClock {
    pub(crate) source: CursorTrajectoryClockSource,
    #[serde(deserialize_with = "nullable")]
    pub(crate) timestamp_quantum_us: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) resolution_bound_us: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) mapping_uncertainty_us: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventKeyData {
    #[serde(deserialize_with = "nullable")]
    pub(crate) down_time_us: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) up_time_us: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) press_duration_us: Option<u64>,
    pub(crate) last_observed_time_us: u64,
    pub(crate) repeat_count: u64,
    pub(crate) completion: EventCompletion,
    #[serde(deserialize_with = "nullable")]
    pub(crate) keyboard_attack_id: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) keyboard_onset_time_us: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) group_id: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) continuation_of: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) ctrl_key: Option<bool>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) alt_key: Option<bool>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) shift_key: Option<bool>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) meta_key: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventClickData {
    pub(crate) button: u8,
    #[serde(deserialize_with = "nullable")]
    pub(crate) gesture_id: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) gesture_type: Option<EventGesture>,
    pub(crate) completion: EventCompletion,
    #[serde(deserialize_with = "nullable")]
    pub(crate) down_time_us: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) up_time_us: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) press_duration_us: Option<u64>,
    pub(crate) last_observed_time_us: u64,
    pub(crate) click_count: u64,
    #[serde(deserialize_with = "nullable")]
    pub(crate) semantic_end_time_us: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) drag_start_time_us: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) pointer_cancelled: Option<bool>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) drop_observed: Option<bool>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) up_point: Option<EventPoint>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) ctrl_key: Option<bool>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) alt_key: Option<bool>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) shift_key: Option<bool>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) meta_key: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventScrollData {
    #[serde(deserialize_with = "nullable")]
    pub(crate) event_type: Option<EventScrollType>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) delta_x: Option<f64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) delta_y: Option<f64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) delta_z: Option<f64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) delta_mode: Option<u8>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) scroll_x: Option<f64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) scroll_y: Option<f64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) ctrl_key: Option<bool>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) alt_key: Option<bool>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) shift_key: Option<bool>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) meta_key: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventPauseData {
    pub(crate) phase: EventPausePhase,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventSignal {
    pub(crate) id: String,
    pub(crate) kind: EventSignalKind,
    pub(crate) start_time_us: u64,
    pub(crate) end_time_us: u64,
    #[serde(deserialize_with = "nullable")]
    pub(crate) point: Option<EventPoint>,
    pub(crate) data: EventSignalData,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub(crate) enum EventSignalData {
    Typing(EventTypingData),
    KeyboardHold(EventKeyboardHoldData),
    CursorIdle(EventCursorIdleData),
    Warning(EventWarningData),
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventKeyboardHoldData {
    pub(crate) privacy: EventPrivacy,
    #[serde(deserialize_with = "nullable")]
    pub(crate) down_time_us: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) up_time_us: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) press_duration_us: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) last_observed_time_us: Option<u64>,
    pub(crate) repeat_count: u64,
    pub(crate) completion: EventCompletion,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventTypingData {
    #[serde(deserialize_with = "nullable")]
    pub(crate) event_count: Option<u64>,
    pub(crate) activity: EventTypingActivity,
    pub(crate) privacy: EventPrivacy,
    #[serde(deserialize_with = "nullable")]
    pub(crate) group_id: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) continuation_of: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventCursorIdleData {
    pub(crate) source: EventCursorSource,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventWarningData {
    #[serde(deserialize_with = "nullable")]
    pub(crate) source: Option<String>,
    pub(crate) code: String,
    pub(crate) severity: EventSeverity,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(untagged)]
pub(crate) enum EventActionUpdate {
    Key(EventKeyUpdate),
    Click(EventClickUpdate),
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventKeyUpdate {
    #[serde(deserialize_with = "nullable")]
    pub(crate) up_time_us: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) press_duration_us: Option<u64>,
    pub(crate) last_observed_time_us: u64,
    pub(crate) repeat_count: u64,
    pub(crate) completion: EventCompletion,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EventClickUpdate {
    #[serde(deserialize_with = "nullable")]
    pub(crate) gesture_type: Option<EventGesture>,
    pub(crate) completion: EventCompletion,
    #[serde(deserialize_with = "nullable")]
    pub(crate) up_time_us: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) press_duration_us: Option<u64>,
    pub(crate) last_observed_time_us: u64,
    #[serde(deserialize_with = "nullable")]
    pub(crate) semantic_end_time_us: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) drag_start_time_us: Option<u64>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) pointer_cancelled: Option<bool>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) drop_observed: Option<bool>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) up_point: Option<EventPoint>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) ctrl_key: Option<bool>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) alt_key: Option<bool>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) shift_key: Option<bool>,
    #[serde(deserialize_with = "nullable")]
    pub(crate) meta_key: Option<bool>,
}
