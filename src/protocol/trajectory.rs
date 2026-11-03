//! Shared cursor-trajectory-v1 records and canonical attachment encoding.

#[cfg(test)]
pub(crate) const CURSOR_TRAJECTORY_FIXTURE_SHA: &str =
    "dc308f3fa394bc44db5686e11c5fb697ef628be5be5a1a09cde174560f4c1294";

mod attachment_validation;
mod attachments;
#[cfg(any(windows, test))]
mod canonical;
mod descriptor;
#[cfg(any(windows, test))]
mod packing;
#[cfg(any(windows, test))]
mod read_grant;
mod records;
#[cfg(any(windows, test))]
mod stream;
mod validation;

pub(crate) use attachment_validation::validate_attachment_message;
pub(crate) use attachments::{
    AppRecordingAttachmentAvailableMessage, AppRecordingAttachmentChunkMessage,
    AppRecordingAttachmentManifestMessage, CURSOR_TRAJECTORY_CAPABILITY,
    ExtensionRecordingAttachmentAckMessage, ExtensionRecordingAttachmentChunkRequestMessage,
    ExtensionRecordingAttachmentManifestRequestMessage, NativeTrajectoryAttachmentCapabilities,
};
#[cfg(any(windows, test))]
pub(crate) use canonical::decode_trajectory_block;
#[cfg(test)]
pub(crate) use canonical::encode_trajectory_block;
pub(crate) use descriptor::validate_trajectory_descriptor;
#[cfg(any(windows, test))]
pub(crate) use packing::CursorTrajectoryPacker;
#[cfg(any(windows, test))]
pub(crate) use read_grant::{
    CursorTrajectoryReadGrant, parse_trajectory_read_grant, trajectory_read_grant_value,
};
#[cfg(any(windows, test))]
pub(crate) use stream::CursorTrajectoryStreamValidator;
pub(crate) use validation::{
    MAX_SAFE_INTEGER, TrajectoryContractError, valid_trajectory_hash, valid_trajectory_id,
};

pub(crate) use records::{
    CursorTrajectoryAlgorithm, CursorTrajectoryDescriptor, CursorTrajectoryEncoding,
    CursorTrajectoryFormat, CursorTrajectoryManifestEntry, CursorTrajectoryReceipt,
    CursorTrajectoryState,
};

#[cfg(any(windows, test))]
pub(crate) use validation::validate_record_shape;

#[cfg(any(windows, test))]
pub(crate) use records::{
    CursorTrajectoryBoundary, CursorTrajectoryButton, CursorTrajectoryClock, CursorTrajectoryEvent,
    CursorTrajectoryGap, CursorTrajectoryGapReason, CursorTrajectorySegmentReason,
};

#[cfg(any(windows, test))]
pub(crate) use records::{
    CursorTrajectoryBlock, CursorTrajectoryBoundaryReason, CursorTrajectoryRecord,
    CursorTrajectorySample, CursorTrajectorySegment,
};
#[cfg(any(windows, test))]
pub(crate) use validation::record_bounds;

#[cfg(any(windows, test))]
pub(crate) use records::CursorTrajectoryClockSource;

#[cfg(test)]
pub(crate) use records::CursorTrajectoryAnchor;

#[cfg(test)]
pub(crate) use canonical::trajectory_fixture;
