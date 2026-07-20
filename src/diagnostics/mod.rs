//! Redacted logging, health checks, doctor output, and support bundles.

mod bundle;
mod doctor;
#[cfg(test)]
mod health;
mod log;
mod observability;
mod recent_errors;
mod runtime_access;
#[cfg(test)]
mod tests;
#[cfg(windows)]
mod windows;
mod writable;

pub(crate) use bundle::{
    BundleCandidate, BundleEntryKind, BundleWriteError, BundleWriteResult, write_runtime_bundle,
};
pub(crate) use doctor::{
    DoctorCategoryStatus, DoctorCheck, DoctorCheckName, DoctorReport, DoctorStatus, doctor_check,
    doctor_check_with_categories, doctor_report,
};
#[cfg(all(windows, not(test)))]
pub(crate) use log::start_process_logger_for_mode_with_options;
#[cfg(not(test))]
pub(crate) use log::{CrashContext, install_panic_logger_for_mode, set_crash_context};
#[cfg(all(windows, not(test)))]
pub(crate) use log::{LogErrorFields, LogLevel, ProcessLogEvent, ProcessLogSink};
#[cfg(all(windows, not(test)))]
pub(crate) use log::{ProcessLogger, ProcessLoggerError};
#[cfg(all(windows, not(test)))]
pub(crate) use observability::logger_startup_warning_line;
pub(crate) use observability::{
    ObservabilitySnapshot, runtime_observability_json, runtime_observability_snapshot,
};
pub(crate) use recent_errors::runtime_recent_errors_json;
pub(crate) use runtime_access::{RuntimeDirectoryAccessReport, runtime_directory_access_report};
pub(crate) use writable::{WritableDirectoryReport, runtime_writable_directory_report};
