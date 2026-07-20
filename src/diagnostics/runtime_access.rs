//! Runtime data directory access policy facts.

use std::{fs, path::PathBuf};

use super::writable::{WritableDirectoryInputs, runtime_directory_plans};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RuntimeDirectoryAccessStatus {
    #[cfg(any(windows, test))]
    Protected,
    #[cfg(any(windows, test))]
    BroadReadAccess,
    #[cfg(any(windows, test))]
    BroadWriteAccess,
    Unknown,
    #[cfg(not(windows))]
    UnsupportedPlatform,
}

impl RuntimeDirectoryAccessStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            #[cfg(any(windows, test))]
            Self::Protected => "protected",
            #[cfg(any(windows, test))]
            Self::BroadReadAccess => "broad-read-access",
            #[cfg(any(windows, test))]
            Self::BroadWriteAccess => "broad-write-access",
            Self::Unknown => "unknown",
            #[cfg(not(windows))]
            Self::UnsupportedPlatform => "unsupported-platform",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RuntimeDirectoryAccessFact {
    pub(crate) category: &'static str,
    pub(crate) status: RuntimeDirectoryAccessStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RuntimeDirectoryAccessReport {
    pub(crate) facts: Vec<RuntimeDirectoryAccessFact>,
}

impl RuntimeDirectoryAccessReport {
    pub(crate) fn repair_required(&self) -> bool {
        self.facts
            .iter()
            .any(|fact| runtime_directory_access_repair_required(fact.status))
    }

    pub(crate) fn unprotected_categories(&self) -> Vec<String> {
        self.facts
            .iter()
            .filter(|fact| runtime_directory_access_repair_required(fact.status))
            .map(|fact| format!("{}={}", fact.category, fact.status.as_str()))
            .collect()
    }
}

pub(crate) fn runtime_directory_access_report() -> RuntimeDirectoryAccessReport {
    runtime_directory_access_report_for_inputs(&WritableDirectoryInputs {
        local_app_data_dir: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
        temp_dir: std::env::temp_dir(),
        process_id: std::process::id(),
    })
}

pub(crate) fn runtime_directory_access_report_for_inputs(
    inputs: &WritableDirectoryInputs,
) -> RuntimeDirectoryAccessReport {
    runtime_directory_access_report_with_probe(inputs, platform_runtime_directory_access_status)
}

pub(crate) fn runtime_directory_access_report_with_probe(
    inputs: &WritableDirectoryInputs,
    mut probe: impl FnMut(&std::path::Path) -> RuntimeDirectoryAccessStatus,
) -> RuntimeDirectoryAccessReport {
    RuntimeDirectoryAccessReport {
        facts: runtime_directory_plans(inputs)
            .into_iter()
            .map(|directory| {
                let status = if fs::create_dir_all(&directory.path).is_ok() {
                    probe(&directory.path)
                } else {
                    RuntimeDirectoryAccessStatus::Unknown
                };
                RuntimeDirectoryAccessFact {
                    category: directory.category,
                    status,
                }
            })
            .collect(),
    }
}

pub(crate) const fn runtime_directory_access_repair_required(
    status: RuntimeDirectoryAccessStatus,
) -> bool {
    match status {
        #[cfg(any(windows, test))]
        RuntimeDirectoryAccessStatus::Protected => false,
        #[cfg(any(windows, test))]
        RuntimeDirectoryAccessStatus::BroadReadAccess
        | RuntimeDirectoryAccessStatus::BroadWriteAccess => true,
        RuntimeDirectoryAccessStatus::Unknown => true,
        #[cfg(not(windows))]
        RuntimeDirectoryAccessStatus::UnsupportedPlatform => true,
    }
}

#[cfg(windows)]
use super::windows as platform_access;

#[cfg(windows)]
fn platform_runtime_directory_access_status(
    path: &std::path::Path,
) -> RuntimeDirectoryAccessStatus {
    platform_access::runtime_access::runtime_directory_access_status(path)
}

#[cfg(not(windows))]
fn platform_runtime_directory_access_status(
    _path: &std::path::Path,
) -> RuntimeDirectoryAccessStatus {
    RuntimeDirectoryAccessStatus::UnsupportedPlatform
}
