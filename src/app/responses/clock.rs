//! Native-host session clock source.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SessionEpochSource {
    ObservedMessage,
    #[cfg(not(test))]
    System,
}

#[cfg(not(test))]
pub(super) fn system_epoch_ms() -> u64 {
    match std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        Ok(duration) => u64::try_from(duration.as_millis())
            .unwrap_or(u64::MAX)
            .max(1),
        Err(_) => 1,
    }
}
