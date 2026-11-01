//! Bounded media chunking, hashing, pending transfers, and reconnect state.

#[cfg(any(windows, test))]
pub(crate) mod browser_delivery;

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
#[cfg(any(windows, test))]
pub(crate) mod events_catalog;
#[cfg(any(windows, test))]
mod events_ledger;
#[cfg(any(windows, test))]
pub(crate) mod events_store;
#[cfg(any(windows, test))]
mod history_block_io;
#[cfg(any(windows, test))]
mod history_files;
#[cfg(any(windows, test))]
mod history_head;
#[cfg(test)]
mod tests;
#[cfg(any(windows, test))]
pub(crate) mod trajectory_catalog;
#[cfg(any(windows, test))]
mod trajectory_crypto;
#[cfg(any(windows, test))]
mod trajectory_index;
#[cfg(any(windows, test))]
mod trajectory_ledger;
#[cfg(any(windows, test))]
pub(crate) mod trajectory_store;
#[cfg(windows)]
mod windows_dpapi;
#[cfg(windows)]
mod windows_files;
