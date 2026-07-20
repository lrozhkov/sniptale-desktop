//! Recording service error mapping.

use super::super::{
    disk_guard::RecordingDiskGuardError, runtime::RecordingRuntimeError,
    session::RecordingStateError, transfer_store::RecordingTransferStoreError,
};

impl From<RecordingStateError> for RecordingRuntimeError {
    fn from(_error: RecordingStateError) -> Self {
        Self::InvalidState
    }
}

impl From<RecordingTransferStoreError> for RecordingRuntimeError {
    fn from(_error: RecordingTransferStoreError) -> Self {
        Self::TransferState
    }
}

impl From<RecordingDiskGuardError> for RecordingRuntimeError {
    fn from(error: RecordingDiskGuardError) -> Self {
        match error {
            RecordingDiskGuardError::DiskPressure { .. } => Self::DiskPressure,
            RecordingDiskGuardError::ProbeFailed | RecordingDiskGuardError::InvalidCapacity => {
                Self::RuntimeFailed
            }
        }
    }
}
