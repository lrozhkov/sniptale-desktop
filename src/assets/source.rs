//! Typed plaintext reader port supplied by a protected media owner.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FinalizedArtifactSourceError {
    #[cfg(test)]
    Unavailable,
    Tampered,
}

/// Reads one bounded plaintext chunk without exposing a protected path or encryption key.
pub(crate) trait FinalizedArtifactSource: Send {
    fn total_bytes(&self) -> u64;
    fn sha256(&self) -> &str;
    fn read_next(
        &mut self,
        maximum_bytes: usize,
    ) -> Result<Option<Vec<u8>>, FinalizedArtifactSourceError>;
}

#[cfg(any(windows, test))]
impl FinalizedArtifactSource for crate::transfer::ProtectedMediaReader {
    fn total_bytes(&self) -> u64 {
        self.total_bytes()
    }

    fn sha256(&self) -> &str {
        self.sha256()
    }

    fn read_next(
        &mut self,
        maximum_bytes: usize,
    ) -> Result<Option<Vec<u8>>, FinalizedArtifactSourceError> {
        self.read_next_plaintext(maximum_bytes)
            .map_err(|_| FinalizedArtifactSourceError::Tampered)
    }
}
