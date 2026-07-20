//! Platform-neutral recording command lifecycle service.

mod control;
mod control_flow;
mod error;
mod finalize;
mod messages;
mod progress_tick;
mod source;
mod start;
mod timebase;
mod timeline;

use self::timebase::RecordingMonotonicClock;
#[cfg(test)]
pub(crate) use self::timebase::RecordingMonotonicClock as TestRecordingMonotonicClock;
use self::timeline::RecordingTimeline;
#[cfg(windows)]
use super::host_runtime::recording_volume_space;
use super::{
    disk_guard::RecordingDiskSpaceProbe,
    progress::RecordingProgressEmitter,
    runtime::{RecordingRuntime, RecordingRuntimeError},
    service_contract::{
        RecordingCommandEvent, RecordingControlRequest, RecordingRunner, RecordingStartPolicy,
        RecordingStartRequest,
    },
    session::RecordingSession,
    transfer_store::RecordingTransferStore,
};
use crate::telemetry::{NoopRecordingTelemetrySource, RecordingTelemetrySource};

#[derive(Debug)]
pub(crate) struct RecordingLifecycleService<R, T = NoopRecordingTelemetrySource> {
    session: RecordingSession,
    transfer_store: RecordingTransferStore,
    runtime: R,
    telemetry: T,
    disk_space_probe: RecordingDiskSpaceProbe,
    timeline: RecordingTimeline,
    start_policy: RecordingStartPolicy,
    open_editor: Option<bool>,
    delivery_route: Option<crate::protocol::DeliveryRoute>,
    telemetry_enabled: bool,
    progress: RecordingProgressEmitter,
    monotonic_clock: RecordingMonotonicClock,
}

impl<R, T> RecordingLifecycleService<R, T>
where
    T: RecordingTelemetrySource,
{
    #[cfg(windows)]
    pub(crate) fn new_with_telemetry(
        transfer_store: RecordingTransferStore,
        runtime: R,
        telemetry: T,
    ) -> Self {
        Self {
            session: RecordingSession::default(),
            transfer_store,
            runtime,
            telemetry,
            disk_space_probe: recording_volume_space,
            timeline: RecordingTimeline::default(),
            start_policy: RecordingStartPolicy::default(),
            open_editor: None,
            delivery_route: None,
            telemetry_enabled: false,
            progress: RecordingProgressEmitter::default(),
            monotonic_clock: RecordingMonotonicClock::default(),
        }
    }
}

#[cfg(test)]
impl<R> RecordingLifecycleService<R> {
    pub(crate) fn new_with_test_inputs(
        transfer_store: RecordingTransferStore,
        runtime: R,
        disk_space_probe: RecordingDiskSpaceProbe,
        monotonic_clock: RecordingMonotonicClock,
    ) -> Self {
        Self {
            session: RecordingSession::default(),
            transfer_store,
            runtime,
            telemetry: NoopRecordingTelemetrySource,
            disk_space_probe,
            timeline: RecordingTimeline::default(),
            start_policy: RecordingStartPolicy::default(),
            open_editor: None,
            delivery_route: None,
            telemetry_enabled: false,
            progress: RecordingProgressEmitter::default(),
            monotonic_clock,
        }
    }
}

#[cfg(test)]
impl<R, T> RecordingLifecycleService<R, T>
where
    T: RecordingTelemetrySource,
{
    pub(crate) fn new_with_test_inputs_and_telemetry(
        transfer_store: RecordingTransferStore,
        runtime: R,
        telemetry: T,
        disk_space_probe: RecordingDiskSpaceProbe,
        monotonic_clock: RecordingMonotonicClock,
    ) -> Self {
        Self {
            session: RecordingSession::default(),
            transfer_store,
            runtime,
            telemetry,
            disk_space_probe,
            timeline: RecordingTimeline::default(),
            start_policy: RecordingStartPolicy::default(),
            open_editor: None,
            delivery_route: None,
            telemetry_enabled: false,
            progress: RecordingProgressEmitter::default(),
            monotonic_clock,
        }
    }
}

impl<R, T> RecordingRunner for RecordingLifecycleService<R, T>
where
    R: RecordingRuntime,
    T: RecordingTelemetrySource,
{
    fn start_recording(
        &mut self,
        request: &RecordingStartRequest,
        policy: RecordingStartPolicy,
    ) -> Result<Vec<RecordingCommandEvent>, RecordingRuntimeError> {
        self.start_recording_lifecycle(request, policy)
    }

    fn control_recording(
        &mut self,
        request: &RecordingControlRequest,
    ) -> Result<Vec<RecordingCommandEvent>, RecordingRuntimeError> {
        self.control_recording_lifecycle(request)
    }
}

impl<R, T> RecordingLifecycleService<R, T>
where
    T: RecordingTelemetrySource,
{
    fn reset(&mut self) {
        self.session = RecordingSession::default();
        self.timeline.reset();
        self.open_editor = None;
        self.delivery_route = None;
        self.telemetry_enabled = false;
        self.progress.reset();
    }

    fn discard_and_reset(&mut self, recording_id: &str) {
        if self.telemetry_enabled {
            self.telemetry.discard_recording(recording_id);
        }
        self.discard_recording_best_effort(recording_id);
        self.reset();
    }

    fn discard_recording_best_effort(&self, recording_id: &str) {
        if self.transfer_store.discard_recording(recording_id).is_err() {
            // Finalization/startup cleanup has no safe user-facing recovery path here; command
            // failure is returned by the primary operation and support logs use coarse categories.
        }
    }
}
