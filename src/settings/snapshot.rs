//! Last accepted extension settings snapshot owner.

use serde_json::Value;

use crate::protocol::SETTINGS_SCHEMA_VERSION;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SettingsSnapshotError {
    InvalidRevision,
    InvalidGeneratedAt,
    InvalidMaxRecordingDuration,
    IncompatibleSchema { received: u16, supported: u16 },
    StoreFailed,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AcceptedSettingsSnapshot {
    pub(crate) controller_lease_id: String,
    pub(crate) revision: String,
    pub(crate) schema_version: u16,
    pub(crate) generated_at_epoch_ms: u64,
    pub(crate) max_recording_duration_ms: u64,
    pub(crate) telemetry: AcceptedRecordingTelemetrySettings,
    pub(crate) settings: Value,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct AcceptedRecordingTelemetrySettings {
    pub(crate) collect_cursor: bool,
    pub(crate) collect_clicks: bool,
    pub(crate) collect_key_events: bool,
    pub(crate) collect_typing_spans: bool,
    pub(crate) collect_static_signals: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SettingsSnapshotStore {
    accepted: Option<AcceptedSettingsSnapshot>,
}

impl SettingsSnapshotStore {
    #[must_use]
    pub(crate) const fn accepted(&self) -> Option<&AcceptedSettingsSnapshot> {
        self.accepted.as_ref()
    }

    pub(crate) fn accept(
        &mut self,
        snapshot: AcceptedSettingsSnapshot,
    ) -> Result<&AcceptedSettingsSnapshot, SettingsSnapshotError> {
        validate_snapshot(&snapshot)?;
        self.accepted = Some(snapshot);
        self.accepted
            .as_ref()
            .ok_or(SettingsSnapshotError::StoreFailed)
    }
}

fn validate_snapshot(snapshot: &AcceptedSettingsSnapshot) -> Result<(), SettingsSnapshotError> {
    if snapshot.revision.is_empty() {
        return Err(SettingsSnapshotError::InvalidRevision);
    }
    if snapshot.generated_at_epoch_ms == 0 {
        return Err(SettingsSnapshotError::InvalidGeneratedAt);
    }
    if snapshot.max_recording_duration_ms == 0 {
        return Err(SettingsSnapshotError::InvalidMaxRecordingDuration);
    }
    if snapshot.schema_version != SETTINGS_SCHEMA_VERSION {
        return Err(SettingsSnapshotError::IncompatibleSchema {
            received: snapshot.schema_version,
            supported: SETTINGS_SCHEMA_VERSION,
        });
    }
    Ok(())
}

pub(crate) fn max_recording_duration_ms_from_minutes(
    minutes: f64,
) -> Result<u64, SettingsSnapshotError> {
    if !minutes.is_finite() || !(1.0..=720.0).contains(&minutes) {
        return Err(SettingsSnapshotError::InvalidMaxRecordingDuration);
    }
    let millis = minutes * 60_000.0;
    format!("{:.0}", millis.round())
        .parse::<u64>()
        .map_err(|_| SettingsSnapshotError::InvalidMaxRecordingDuration)
}
