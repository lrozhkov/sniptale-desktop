#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TransferError {
    EmptyTransfer,
    #[cfg(test)]
    FileIo,
    InvalidLimit,
    ProtectedMedia,
    #[cfg(any(windows, test))]
    InvalidTransferId,
    TotalBytesTooLarge {
        total_bytes: u64,
        limit: u64,
    },
    ChunkIndexOutOfBounds {
        index: u64,
        chunk_count: u64,
    },
    ChunkSizeMismatch {
        expected: usize,
        actual: usize,
    },
    ChunkRawBytesTooLarge {
        raw_bytes: usize,
        limit: usize,
    },
    Base64TooLarge {
        encoded_chars: usize,
        limit: usize,
    },
    JsonMessageTooLarge {
        json_bytes: usize,
        limit: usize,
    },
    #[cfg(any(windows, test))]
    OutOfOrderChunk {
        expected: u64,
        actual: u64,
    },
    #[cfg(test)]
    AckBeforeComplete {
        next_missing_chunk_index: u64,
        chunk_count: u64,
    },
    #[cfg(any(windows, test))]
    TransferAlreadyTerminal,
}
