//! Bounded media chunking, hashing, pending transfers, and reconnect state.

#[cfg(any(windows, test))]
pub(crate) mod chunks;
#[cfg(any(windows, test))]
pub(crate) mod error;
#[cfg(test)]
pub(crate) mod file;
#[cfg(any(windows, test))]
pub(crate) mod hashing;
#[cfg(any(windows, test))]
pub(crate) mod identity;
#[cfg(any(windows, test))]
pub(crate) mod lifecycle;
#[cfg(any(windows, test))]
pub(crate) mod messages;
#[cfg(any(windows, test))]
pub(crate) mod pending;
#[cfg(any(windows, test))]
pub(crate) mod protected_file;
#[cfg(any(windows, test))]
pub(crate) mod protected_key;
#[cfg(any(windows, test))]
pub(crate) mod protected_read;
#[cfg(any(windows, test))]
pub(crate) use protected_read::ProtectedMediaReader;
#[cfg(test)]
mod tests;
#[cfg(windows)]
mod windows_dpapi;
