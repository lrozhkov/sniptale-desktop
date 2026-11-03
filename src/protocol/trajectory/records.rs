//! Closed platform-neutral trajectory record shapes fixed by ADR 0006.

use serde::{Deserialize, Serialize};

#[cfg(any(windows, test))]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CursorTrajectoryBlock {
    pub(crate) format_version: u16,
    pub(crate) recording_id: String,
    pub(crate) trajectory_id: String,
    pub(crate) sequence: u64,
    pub(crate) start_time_us: u64,
    pub(crate) end_time_us: u64,
    pub(crate) records: Vec<CursorTrajectoryRecord>,
}

#[cfg(any(windows, test))]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum CursorTrajectoryRecord {
    Segment(CursorTrajectorySegment),
    Sample(CursorTrajectorySample),
    Boundary(CursorTrajectoryBoundary),
    Gap(CursorTrajectoryGap),
}

#[cfg(any(windows, test))]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CursorTrajectorySegment {
    pub(crate) segment_id: u64,
    pub(crate) active_time_us: u64,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) clock: CursorTrajectoryClock,
    pub(crate) reason: CursorTrajectorySegmentReason,
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CursorTrajectorySegmentReason {
    Start,
    Resume,
    Visibility,
    Observation,
    Geometry,
    ClockDiscontinuity,
    ZeroTimeDiscontinuity,
    AfterGap,
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CursorTrajectoryClock {
    pub(crate) source: CursorTrajectoryClockSource,
    pub(crate) timestamp_quantum_us: Option<u64>,
    pub(crate) resolution_bound_us: Option<u64>,
    pub(crate) mapping_uncertainty_us: Option<u64>,
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CursorTrajectoryClockSource {
    WindowsMessageTime,
    DomEventTime,
}

#[cfg(any(windows, test))]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CursorTrajectorySample {
    pub(crate) segment_id: u64,
    pub(crate) observation_seq: u64,
    pub(crate) active_time_us: u64,
    pub(crate) x_q8: Option<u64>,
    pub(crate) y_q8: Option<u64>,
    pub(crate) visible: bool,
    pub(crate) observed: bool,
    pub(crate) event: CursorTrajectoryEvent,
    pub(crate) button: CursorTrajectoryButton,
    pub(crate) related_observation_seq: Option<u64>,
    pub(crate) anchors: Vec<CursorTrajectoryAnchor>,
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CursorTrajectoryEvent {
    Move,
    Down,
    Up,
    Click,
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CursorTrajectoryButton {
    None,
    Left,
    Middle,
    Right,
    X1,
    X2,
}

// Declaration order is the contract's ASCII order, not event or presentation priority.
#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CursorTrajectoryAnchor {
    AfterButton,
    BeforeButton,
    Discontinuity,
    Gap,
    Geometry,
    Observation,
    Pause,
    Resume,
    Start,
    Stop,
    Visibility,
}

#[cfg(any(windows, test))]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CursorTrajectoryBoundary {
    pub(crate) segment_id: u64,
    pub(crate) active_time_us: u64,
    pub(crate) reason: CursorTrajectoryBoundaryReason,
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CursorTrajectoryBoundaryReason {
    Pause,
    Stop,
    Visibility,
    Observation,
    Geometry,
    ClockDiscontinuity,
    ZeroTimeDiscontinuity,
}

#[cfg(any(windows, test))]
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CursorTrajectoryGap {
    pub(crate) segment_id: u64,
    pub(crate) from_time_us: u64,
    pub(crate) to_time_us: Option<u64>,
    pub(crate) first_lost_observation_seq: Option<u64>,
    pub(crate) last_lost_observation_seq: Option<u64>,
    pub(crate) reason: CursorTrajectoryGapReason,
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CursorTrajectoryGapReason {
    InputOverflow,
    WriterBackpressure,
    SourceUnavailable,
    StorageFailure,
    ClockDiscontinuity,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CursorTrajectoryDescriptor {
    pub(crate) format: CursorTrajectoryFormat,
    pub(crate) format_version: u16,
    pub(crate) algorithm: CursorTrajectoryAlgorithm,
    pub(crate) encoding: CursorTrajectoryEncoding,
    pub(crate) trajectory_id: String,
    pub(crate) state: CursorTrajectoryState,
    pub(crate) finalized: bool,
    pub(crate) total_blocks: u64,
    pub(crate) total_records: u64,
    pub(crate) total_samples: u64,
    pub(crate) total_bytes: u64,
    pub(crate) stream_sha256: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum CursorTrajectoryFormat {
    #[serde(rename = "cursor-trajectory-v1")]
    V1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum CursorTrajectoryAlgorithm {
    #[serde(rename = "sed-window-v1")]
    SedWindowV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum CursorTrajectoryEncoding {
    #[serde(rename = "json-ascii-int-v1")]
    JsonAsciiIntV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CursorTrajectoryState {
    Complete,
    Incomplete,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CursorTrajectoryManifestEntry {
    pub(crate) sequence: u64,
    pub(crate) start_time_us: u64,
    pub(crate) end_time_us: u64,
    pub(crate) record_count: u16,
    pub(crate) sample_count: u16,
    pub(crate) byte_length: u32,
    pub(crate) block_sha256: String,
    pub(crate) prefix_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CursorTrajectoryReceipt {
    pub(crate) version: u16,
    pub(crate) durable: bool,
    pub(crate) format: CursorTrajectoryFormat,
    pub(crate) state: CursorTrajectoryState,
    pub(crate) total_blocks: u64,
    pub(crate) total_records: u64,
    pub(crate) total_samples: u64,
    pub(crate) total_bytes: u64,
    pub(crate) stream_sha256: String,
}
