use super::super::{
    error::TransferError,
    pending::{PendingTransfer, TerminalTransferState},
};
use proptest::{prelude::*, test_runner::TestCaseError};

proptest! {
    #[test]
    fn pending_transfer_accepts_in_order_state_progression(chunk_total in 1_u64..64) {
        let mut pending = PendingTransfer::new("transfer-1", chunk_total)
            .map_err(|error| TestCaseError::fail(format!("{error:?}")))?;

        for index in 0..chunk_total {
            prop_assert_eq!(pending.next_missing_chunk_index(), index);
            pending
                .record_verified_chunk(index)
                .map_err(|error| TestCaseError::fail(format!("{error:?}")))?;
        }
        prop_assert!(pending.is_complete());
        pending
            .ack()
            .map_err(|error| TestCaseError::fail(format!("{error:?}")))?;
        prop_assert_eq!(pending.terminal_state(), Some(&TerminalTransferState::Acked));
    }
}

#[test]
fn pending_transfer_tracks_resume_and_terminal_states() -> Result<(), TransferError> {
    let mut pending = PendingTransfer::new("recording-1", 3)?;

    assert_eq!(pending.next_missing_chunk_index(), 0);
    pending.record_verified_chunk(0)?;
    pending.record_verified_chunk(0)?;
    assert_eq!(pending.next_missing_chunk_index(), 1);
    assert!(matches!(
        pending.record_verified_chunk(2),
        Err(TransferError::OutOfOrderChunk {
            expected: 1,
            actual: 2
        })
    ));
    pending.record_verified_chunk(1)?;
    pending.record_verified_chunk(2)?;
    assert!(pending.is_complete());
    pending.ack()?;

    assert_eq!(
        pending.terminal_state(),
        Some(&TerminalTransferState::Acked)
    );
    assert!(matches!(
        pending.record_verified_chunk(2),
        Err(TransferError::TransferAlreadyTerminal)
    ));
    Ok(())
}

#[test]
fn pending_transfer_rejects_and_expires_without_ack() -> Result<(), TransferError> {
    let mut rejected = PendingTransfer::new("capture-1", 1)?;
    rejected.reject(true)?;
    assert_eq!(
        rejected.terminal_state(),
        Some(&TerminalTransferState::Rejected {
            retain_for_diagnostics: true
        })
    );

    let mut expired = PendingTransfer::new("capture-2", 1)?;
    expired.expire()?;
    assert_eq!(
        expired.terminal_state(),
        Some(&TerminalTransferState::Expired)
    );

    let mut incomplete = PendingTransfer::new("capture-3", 2)?;
    assert!(matches!(
        incomplete.ack(),
        Err(TransferError::AckBeforeComplete { .. })
    ));
    Ok(())
}
