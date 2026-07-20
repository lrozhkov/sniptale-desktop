//! Pending transfer reconnect and resume state owner.

use super::error::TransferError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TerminalTransferState {
    Acked,
    Rejected {
        retain_for_diagnostics: bool,
    },
    #[cfg(test)]
    Expired,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PendingTransfer {
    transfer_id: String,
    chunk_count: u64,
    next_missing_chunk_index: u64,
    terminal_state: Option<TerminalTransferState>,
}

impl PendingTransfer {
    #[cfg(test)]
    pub(crate) fn new(
        transfer_id: impl Into<String>,
        chunk_count: u64,
    ) -> Result<Self, TransferError> {
        Self::resume(transfer_id, chunk_count, 0)
    }

    pub(crate) fn resume(
        transfer_id: impl Into<String>,
        chunk_count: u64,
        next_missing_chunk_index: u64,
    ) -> Result<Self, TransferError> {
        let transfer_id = transfer_id.into();
        if transfer_id.is_empty() {
            return Err(TransferError::InvalidTransferId);
        }
        if chunk_count == 0 {
            return Err(TransferError::EmptyTransfer);
        }
        if next_missing_chunk_index > chunk_count {
            return Err(TransferError::ChunkIndexOutOfBounds {
                index: next_missing_chunk_index,
                chunk_count,
            });
        }
        Ok(Self {
            transfer_id,
            chunk_count,
            next_missing_chunk_index,
            terminal_state: None,
        })
    }

    #[must_use]
    pub(crate) const fn next_missing_chunk_index(&self) -> u64 {
        self.next_missing_chunk_index
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) const fn terminal_state(&self) -> Option<&TerminalTransferState> {
        self.terminal_state.as_ref()
    }

    #[must_use]
    pub(crate) const fn is_complete(&self) -> bool {
        self.next_missing_chunk_index == self.chunk_count
    }

    pub(crate) fn ensure_expected_chunk(&self, index: u64) -> Result<(), TransferError> {
        self.ensure_active()?;
        if index >= self.chunk_count {
            return Err(TransferError::ChunkIndexOutOfBounds {
                index,
                chunk_count: self.chunk_count,
            });
        }
        if index != self.next_missing_chunk_index {
            return Err(TransferError::OutOfOrderChunk {
                expected: self.next_missing_chunk_index,
                actual: index,
            });
        }
        Ok(())
    }

    pub(crate) fn record_next_chunk(&mut self, index: u64) -> Result<(), TransferError> {
        self.ensure_expected_chunk(index)?;
        self.next_missing_chunk_index += 1;
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn record_verified_chunk(&mut self, index: u64) -> Result<(), TransferError> {
        self.ensure_active()?;
        if index >= self.chunk_count {
            return Err(TransferError::ChunkIndexOutOfBounds {
                index,
                chunk_count: self.chunk_count,
            });
        }
        if index < self.next_missing_chunk_index {
            return Ok(());
        }
        if index != self.next_missing_chunk_index {
            return Err(TransferError::OutOfOrderChunk {
                expected: self.next_missing_chunk_index,
                actual: index,
            });
        }
        self.next_missing_chunk_index += 1;
        Ok(())
    }

    pub(crate) fn acknowledge_for_cleanup(&mut self) -> Result<(), TransferError> {
        self.ensure_active()?;
        self.terminal_state = Some(TerminalTransferState::Acked);
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn ack(&mut self) -> Result<(), TransferError> {
        self.ensure_active()?;
        if !self.is_complete() {
            return Err(TransferError::AckBeforeComplete {
                next_missing_chunk_index: self.next_missing_chunk_index,
                chunk_count: self.chunk_count,
            });
        }
        self.terminal_state = Some(TerminalTransferState::Acked);
        Ok(())
    }

    pub(crate) fn reject(&mut self, retain_for_diagnostics: bool) -> Result<(), TransferError> {
        self.ensure_active()?;
        self.terminal_state = Some(TerminalTransferState::Rejected {
            retain_for_diagnostics,
        });
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn expire(&mut self) -> Result<(), TransferError> {
        self.ensure_active()?;
        self.terminal_state = Some(TerminalTransferState::Expired);
        Ok(())
    }

    fn ensure_active(&self) -> Result<(), TransferError> {
        if self.terminal_state.is_some() {
            Err(TransferError::TransferAlreadyTerminal)
        } else {
            Ok(())
        }
    }
}
