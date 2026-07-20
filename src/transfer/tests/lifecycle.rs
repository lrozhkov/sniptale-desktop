use super::super::{
    error::TransferError,
    lifecycle::{
        TransferLifecycleError, TransferLifecyclePolicy, TransferLifecycleSnapshot,
        TransferLifecycleState, TransferProgress, TransferRecoveryDecision,
        cleanup_recovery_decision,
    },
};
use crate::protocol::CHUNK_RAW_BYTES;

#[test]
fn transfer_lifecycle_policy_handles_common_capture_and_recording_recovery() {
    let policy = TransferLifecyclePolicy {
        writing_ttl_ms: 500,
        finalized_ttl_ms: 60_000,
    };
    let capture = TransferLifecycleSnapshot {
        transfer_id: "capture-1",
        state: TransferLifecycleState::Finalized,
        updated_at_ms: 2_000,
        total_bytes: Some(CHUNK_RAW_BYTES + 1),
        next_missing_chunk_index: 0,
        retain_rejected_until_ms: None,
    };
    let recording = TransferLifecycleSnapshot {
        transfer_id: "recording-1",
        state: TransferLifecycleState::Writing,
        updated_at_ms: 1_000,
        total_bytes: None,
        next_missing_chunk_index: 0,
        retain_rejected_until_ms: None,
    };
    let rejected = TransferLifecycleSnapshot {
        transfer_id: "recording-2",
        state: TransferLifecycleState::Rejected,
        updated_at_ms: 3_000,
        total_bytes: Some(11),
        next_missing_chunk_index: 0,
        retain_rejected_until_ms: Some(10_000),
    };

    assert_eq!(
        capture.recovery_decision(3_000, policy),
        Ok(TransferRecoveryDecision::ReportPending)
    );
    assert_eq!(
        capture.recovery_decision(70_001, policy),
        Ok(TransferRecoveryDecision::Delete)
    );
    assert_eq!(
        recording.recovery_decision(1_500, policy),
        Ok(TransferRecoveryDecision::KeepWriting)
    );
    assert_eq!(
        recording.recovery_decision(1_501, policy),
        Ok(TransferRecoveryDecision::Delete)
    );
    assert_eq!(
        rejected.recovery_decision(4_000, policy),
        Ok(TransferRecoveryDecision::RetainRejected)
    );
    assert_eq!(
        rejected.recovery_decision(10_000, policy),
        Ok(TransferRecoveryDecision::Delete)
    );
}

#[test]
fn transfer_lifecycle_snapshot_tracks_chunk_ack_and_reject_progress() {
    let snapshot = TransferLifecycleSnapshot {
        transfer_id: "capture-1",
        state: TransferLifecycleState::Finalized,
        updated_at_ms: 2_000,
        total_bytes: Some(CHUNK_RAW_BYTES + 1),
        next_missing_chunk_index: 0,
        retain_rejected_until_ms: None,
    };

    assert_eq!(
        snapshot.record_chunk_read(0, 3_000),
        Ok(TransferProgress {
            next_missing_chunk_index: 1,
            is_complete: false,
        })
    );
    assert_eq!(
        snapshot.record_chunk_read(2, 3_000),
        Err(TransferLifecycleError::Transfer(
            TransferError::ChunkIndexOutOfBounds {
                index: 2,
                chunk_count: 2,
            }
        ))
    );
    assert_eq!(snapshot.acknowledge_for_cleanup(3_000), Ok(()));
    assert_eq!(snapshot.reject_for_terminal(3_000, true), Ok(()));

    let acked = TransferLifecycleSnapshot {
        state: TransferLifecycleState::Acked,
        ..snapshot
    };
    assert_eq!(
        acked.record_chunk_read(0, 3_000),
        Err(TransferLifecycleError::InvalidState)
    );
}

#[test]
fn transfer_lifecycle_cleanup_policy_deletes_writing_only_when_requested() {
    assert_eq!(
        cleanup_recovery_decision(TransferRecoveryDecision::KeepWriting, true, true),
        TransferRecoveryDecision::Delete
    );
    assert_eq!(
        cleanup_recovery_decision(TransferRecoveryDecision::KeepWriting, true, false),
        TransferRecoveryDecision::KeepWriting
    );
    assert_eq!(
        cleanup_recovery_decision(TransferRecoveryDecision::ReportPending, false, true),
        TransferRecoveryDecision::ReportPending
    );
    assert_eq!(
        cleanup_recovery_decision(TransferRecoveryDecision::RetainRejected, false, true),
        TransferRecoveryDecision::RetainRejected
    );
}

#[test]
fn transfer_cleanup_scan_budget_classifies_batch_without_payload_access() {
    let policy = TransferLifecyclePolicy {
        writing_ttl_ms: 500,
        finalized_ttl_ms: 500,
    };
    let snapshots = [
        TransferLifecycleSnapshot {
            transfer_id: "pending",
            state: TransferLifecycleState::Finalized,
            updated_at_ms: 2_800,
            total_bytes: Some(11),
            next_missing_chunk_index: 0,
            retain_rejected_until_ms: None,
        },
        TransferLifecycleSnapshot {
            transfer_id: "expired",
            state: TransferLifecycleState::Finalized,
            updated_at_ms: 2_000,
            total_bytes: Some(11),
            next_missing_chunk_index: 0,
            retain_rejected_until_ms: None,
        },
        TransferLifecycleSnapshot {
            transfer_id: "writing",
            state: TransferLifecycleState::Writing,
            updated_at_ms: 2_800,
            total_bytes: None,
            next_missing_chunk_index: 0,
            retain_rejected_until_ms: None,
        },
        TransferLifecycleSnapshot {
            transfer_id: "rejected",
            state: TransferLifecycleState::Rejected,
            updated_at_ms: 2_100,
            total_bytes: Some(11),
            next_missing_chunk_index: 0,
            retain_rejected_until_ms: Some(3_200),
        },
    ];
    let mut counts = (0, 0, 0, 0);

    for snapshot in snapshots {
        match cleanup_recovery_decision(
            snapshot
                .recovery_decision(3_000, policy)
                .unwrap_or(TransferRecoveryDecision::Delete),
            snapshot.state == TransferLifecycleState::Writing,
            false,
        ) {
            TransferRecoveryDecision::KeepWriting => counts.0 += 1,
            TransferRecoveryDecision::ReportPending => counts.1 += 1,
            TransferRecoveryDecision::RetainRejected => counts.2 += 1,
            TransferRecoveryDecision::Delete => counts.3 += 1,
        }
    }

    assert_eq!(counts, (1, 1, 1, 1));
}
