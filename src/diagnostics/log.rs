//! Redacted structured logging owner.

pub(super) mod crash;
pub(super) mod file;
pub(super) mod record;
pub(super) mod redaction;
pub(super) mod runtime;
pub(super) mod setup;
#[cfg(all(windows, not(test)))]
mod windows_exception;

#[cfg(not(test))]
pub(crate) use crash::{CrashContext, install_panic_logger_for_mode, set_crash_context};
#[cfg(all(windows, not(test)))]
pub(crate) use record::{LogErrorFields, LogLevel};
pub(crate) use redaction::{DiagnosticOutput, redact_for_output};
#[cfg(all(windows, not(test)))]
pub(crate) use runtime::start_process_logger_for_mode_with_options;
#[cfg(all(windows, not(test)))]
pub(crate) use runtime::{ProcessLogEvent, ProcessLogSink};
#[cfg(all(windows, not(test)))]
pub(crate) use runtime::{ProcessLogger, ProcessLoggerError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LogRetentionPolicy {
    pub(crate) max_file_bytes: u64,
    pub(crate) max_files: usize,
    pub(crate) retention_days: u16,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LogFileSnapshot {
    pub(crate) file_name: String,
    pub(crate) created_at_epoch_ms: u64,
    pub(crate) size_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LogRotationPlan {
    pub(crate) rotate_active: bool,
    pub(crate) keep_file_names: Vec<String>,
    pub(crate) delete_file_names: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LogRotationError {
    MaxFileBytes,
    MaxFiles,
    RetentionDays,
    FileName,
}

const DEFAULT_MAX_LOG_BYTES: u64 = 10 * 1024 * 1024;
const DEFAULT_MAX_LOG_FILES: usize = 10;
const DEFAULT_RETENTION_DAYS: u16 = 14;
const DAY_MS: u64 = 24 * 60 * 60 * 1_000;

pub(crate) const fn default_log_retention_policy() -> LogRetentionPolicy {
    LogRetentionPolicy {
        max_file_bytes: DEFAULT_MAX_LOG_BYTES,
        max_files: DEFAULT_MAX_LOG_FILES,
        retention_days: DEFAULT_RETENTION_DAYS,
    }
}

pub(crate) fn plan_log_rotation(
    policy: LogRetentionPolicy,
    active_size_bytes: u64,
    now_epoch_ms: u64,
    files: &[LogFileSnapshot],
) -> Result<LogRotationPlan, LogRotationError> {
    validate_log_retention_policy(policy)?;
    for file in files {
        validate_log_file_name(&file.file_name)?;
    }

    let rotate_active = active_size_bytes >= policy.max_file_bytes;
    let retention_ms = u64::from(policy.retention_days) * DAY_MS;
    let retention_cutoff = now_epoch_ms.saturating_sub(retention_ms);
    let mut keep = Vec::new();
    let mut delete = Vec::new();
    let mut retained: Vec<&LogFileSnapshot> = files
        .iter()
        .filter(|file| {
            if file.created_at_epoch_ms < retention_cutoff {
                delete.push(file.file_name.clone());
                false
            } else {
                true
            }
        })
        .collect();
    retained.sort_by(|left, right| {
        right
            .created_at_epoch_ms
            .cmp(&left.created_at_epoch_ms)
            .then_with(|| right.file_name.cmp(&left.file_name))
    });
    for (index, file) in retained.into_iter().enumerate() {
        if index < policy.max_files {
            keep.push(file.file_name.clone());
        } else {
            delete.push(file.file_name.clone());
        }
    }
    delete.sort();
    Ok(LogRotationPlan {
        rotate_active,
        keep_file_names: keep,
        delete_file_names: delete,
    })
}

const fn validate_log_retention_policy(policy: LogRetentionPolicy) -> Result<(), LogRotationError> {
    if policy.max_file_bytes == 0 {
        return Err(LogRotationError::MaxFileBytes);
    }
    if policy.max_files == 0 {
        return Err(LogRotationError::MaxFiles);
    }
    if policy.retention_days == 0 {
        return Err(LogRotationError::RetentionDays);
    }
    Ok(())
}

fn validate_log_file_name(name: &str) -> Result<(), LogRotationError> {
    let valid = !name.is_empty()
        && !name.contains(['/', '\\', ':'])
        && std::path::Path::new(name)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|extension| extension.eq_ignore_ascii_case("jsonl"))
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(LogRotationError::FileName)
    }
}
