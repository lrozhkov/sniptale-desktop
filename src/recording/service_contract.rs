//! Platform-neutral recording command service contract.

#[cfg(any(windows, test))]
use crate::protocol::{
    DeliveryRoute, NativeEffectiveQualitySettings, NativeRecordingSource,
    NativeRecordingTelemetrySnapshot, NativeRecordingTimebase, NativeRequestedQualitySettings,
    OperationAuthority, RecordingControl, RecordingProgressStatus,
};
#[cfg(any(windows, test))]
use crate::recording::{PendingRecordingTransfer, runtime::RecordingRuntimeError};
use crate::telemetry::RecordingTelemetryPolicy;

pub(crate) const DEFAULT_MAX_RECORDING_DURATION_MS: u64 = 4 * 60 * 60 * 1_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RecordingStartPolicy {
    pub(crate) max_duration_ms: u64,
    pub(crate) telemetry: RecordingTelemetryPolicy,
}

/// Immutable recording admission snapshot produced by a boundary adapter.
///
/// It contains no extension command DTO. The authority and route are frozen before recording work,
/// while `delivery_session` remains transient and is never persisted in media metadata.
#[cfg(any(windows, test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecordingStartRequest {
    pub(crate) operation_id: String,
    pub(crate) source: NativeRecordingSource,
    pub(crate) authority: OperationAuthority,
    pub(crate) delivery_route: DeliveryRoute,
    pub(crate) settings_revision: String,
    pub(crate) requested_quality: NativeRequestedQualitySettings,
    pub(crate) include_cursor: bool,
    pub(crate) open_editor: bool,
    pub(crate) requested_at_epoch_ms: u64,
    pub(crate) delivery_session: RecordingDeliverySession,
}

/// Session-scoped delivery identity, intentionally separate from durable operation state.
#[cfg(any(windows, test))]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RecordingDeliverySession {
    pub(crate) controller_lease_id: Option<String>,
}

#[cfg(any(windows, test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecordingControlRequest {
    pub(crate) operation_id: String,
    pub(crate) recording_id: String,
    pub(crate) control: RecordingControl,
    pub(crate) requested_at_epoch_ms: u64,
    pub(crate) delivery_session: RecordingDeliverySession,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(any(windows, test))]
pub(crate) enum RecordingCommandEvent {
    Started(RecordingStartedEvent),
    Progress(RecordingProgressEvent),
    Stopped(RecordingStoppedEvent),
    ManualFinalized(ManualRecordingFinalizedEvent),
}

#[cfg(any(windows, test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecordingStartedEvent {
    pub(crate) recording_id: String,
    pub(crate) source: NativeRecordingSource,
    pub(crate) timebase: NativeRecordingTimebase,
    pub(crate) requested_settings_revision: String,
    pub(crate) requested_quality: NativeRequestedQualitySettings,
    pub(crate) effective_quality: NativeEffectiveQualitySettings,
    pub(crate) started_at_epoch_ms: u64,
}

#[cfg(any(windows, test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecordingProgressEvent {
    pub(crate) recording_id: String,
    pub(crate) duration_ms: u64,
    pub(crate) bytes_written: u64,
    pub(crate) status: RecordingProgressStatus,
}

#[cfg(any(windows, test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecordingStoppedEvent {
    pub(crate) pending: PendingRecordingTransfer,
    pub(crate) telemetry: Option<NativeRecordingTelemetrySnapshot>,
}

#[cfg(any(windows, test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ManualRecordingFinalizedEvent {
    pub(crate) recording_id: String,
    pub(crate) asset_job_id: String,
}

impl Default for RecordingStartPolicy {
    fn default() -> Self {
        Self {
            max_duration_ms: DEFAULT_MAX_RECORDING_DURATION_MS,
            telemetry: RecordingTelemetryPolicy::default(),
        }
    }
}

#[cfg(any(windows, test))]
pub(crate) trait RecordingRunner {
    fn start_recording(
        &mut self,
        request: &RecordingStartRequest,
        policy: RecordingStartPolicy,
    ) -> Result<Vec<RecordingCommandEvent>, RecordingRuntimeError>;

    fn control_recording(
        &mut self,
        request: &RecordingControlRequest,
    ) -> Result<Vec<RecordingCommandEvent>, RecordingRuntimeError>;
}
