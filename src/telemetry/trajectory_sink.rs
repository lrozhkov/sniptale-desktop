//! Narrow orchestration port: telemetry produces blocks, transfer owns their persistence.

use super::{RecordingFrameGeometry, RecordingPointMapping};
use crate::protocol::{DeliveryRoute, trajectory::CursorTrajectoryDescriptor};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecordingTrajectoryContext {
    pub(crate) frame: RecordingFrameGeometry,
    pub(crate) point_mapping: RecordingPointMapping,
    pub(crate) delivery_route: DeliveryRoute,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CursorTrajectorySinkError {
    Backpressure,
    Storage,
    Retired,
}

pub(crate) trait CursorTrajectorySink: std::fmt::Debug + Send {
    /// Must not wait for disk or an available queue slot on the hook-pumping thread.
    fn try_append(&mut self, canonical_block: Vec<u8>) -> Result<(), CursorTrajectorySinkError>;

    /// Called after hooks are removed. A timeout returns the actual committed descriptor,
    /// which stays unfinalized until the owned writer completes its durable update.
    fn finish(&mut self, incomplete: bool) -> CursorTrajectoryDescriptor;

    /// Does not report success until the writer has retired and erased private state.
    fn erase(&mut self) -> Result<(), CursorTrajectorySinkError>;
}

pub(crate) trait CursorTrajectorySinkFactory: std::fmt::Debug + Send {
    fn create(
        &mut self,
        recording_id: &str,
        trajectory_id: &str,
        context: &RecordingTrajectoryContext,
    ) -> Result<Box<dyn CursorTrajectorySink>, CursorTrajectorySinkError>;
}
