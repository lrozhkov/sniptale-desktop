//! Managed log file lifecycle.

use std::{
    error::Error,
    fmt,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use super::{
    LogFileSnapshot, LogRetentionPolicy, LogRotationError, default_log_retention_policy,
    plan_log_rotation,
};

const MANAGED_LOG_PREFIX: &str = "sniptale-native-";
const MANAGED_LOG_EXTENSION: &str = "jsonl";
const CREATE_ATTEMPTS: u16 = 100;

#[derive(Debug)]
pub(crate) struct PreparedLogFile {
    pub(crate) file: File,
    pub(crate) outcome: LogFileLifecycleOutcome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LogFileLifecycleOutcome {
    pub(crate) active_file_name: String,
    pub(crate) rotate_active: bool,
    pub(crate) kept_file_names: Vec<String>,
    pub(crate) deleted_file_names: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LogFileLifecycleError {
    Io,
    Clock,
    Rotation(LogRotationError),
    CreateCollision,
}

impl fmt::Display for LogFileLifecycleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io => formatter.write_str("managed log file IO failed"),
            Self::Clock => formatter.write_str("managed log file timestamp failed"),
            Self::Rotation(error) => write!(formatter, "managed log rotation failed: {error:?}"),
            Self::CreateCollision => formatter.write_str("managed log file name collision"),
        }
    }
}

impl Error for LogFileLifecycleError {}

pub(crate) fn prepare_managed_log_file(
    directory: &Path,
    now_epoch_ms: u64,
    process_id: u32,
) -> Result<PreparedLogFile, LogFileLifecycleError> {
    prepare_managed_log_file_with_policy(
        directory,
        now_epoch_ms,
        process_id,
        default_log_retention_policy(),
    )
}

pub(crate) fn prepare_managed_log_file_with_policy(
    directory: &Path,
    now_epoch_ms: u64,
    process_id: u32,
    policy: LogRetentionPolicy,
) -> Result<PreparedLogFile, LogFileLifecycleError> {
    fs::create_dir_all(directory).map_err(|_| LogFileLifecycleError::Io)?;
    let snapshots = managed_log_snapshots(directory)?;
    let rotation = plan_log_rotation(policy, 0, now_epoch_ms, &snapshots)
        .map_err(LogFileLifecycleError::Rotation)?;
    for file_name in &rotation.delete_file_names {
        let _ = fs::remove_file(directory.join(file_name));
    }
    let (active_file_name, file) = create_active_log_file(directory, now_epoch_ms, process_id)?;
    Ok(PreparedLogFile {
        file,
        outcome: LogFileLifecycleOutcome {
            active_file_name,
            rotate_active: rotation.rotate_active,
            kept_file_names: rotation.keep_file_names,
            deleted_file_names: rotation.delete_file_names,
        },
    })
}

fn managed_log_snapshots(directory: &Path) -> Result<Vec<LogFileSnapshot>, LogFileLifecycleError> {
    let mut snapshots = Vec::new();
    for entry in fs::read_dir(directory).map_err(|_| LogFileLifecycleError::Io)? {
        let entry = entry.map_err(|_| LogFileLifecycleError::Io)?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if !is_managed_log_file_name(&file_name) {
            continue;
        }
        let metadata = entry.metadata().map_err(|_| LogFileLifecycleError::Io)?;
        if !metadata.is_file() {
            continue;
        }
        let modified = metadata
            .modified()
            .map_err(|_| LogFileLifecycleError::Clock)?;
        let created_at_epoch_ms = modified
            .duration_since(UNIX_EPOCH)
            .map_err(|_| LogFileLifecycleError::Clock)?
            .as_millis()
            .try_into()
            .map_err(|_| LogFileLifecycleError::Clock)?;
        snapshots.push(LogFileSnapshot {
            file_name,
            created_at_epoch_ms,
            size_bytes: metadata.len(),
        });
    }
    Ok(snapshots)
}

fn create_active_log_file(
    directory: &Path,
    now_epoch_ms: u64,
    process_id: u32,
) -> Result<(String, File), LogFileLifecycleError> {
    for attempt in 0..CREATE_ATTEMPTS {
        let file_name = managed_log_file_name(now_epoch_ms, process_id, attempt);
        let path: PathBuf = directory.join(&file_name);
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(file) => return Ok((file_name, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return Err(LogFileLifecycleError::Io),
        }
    }
    Err(LogFileLifecycleError::CreateCollision)
}

fn managed_log_file_name(now_epoch_ms: u64, process_id: u32, attempt: u16) -> String {
    format!("{MANAGED_LOG_PREFIX}{now_epoch_ms}-{process_id}-{attempt}.jsonl")
}

fn is_managed_log_file_name(file_name: &str) -> bool {
    file_name.starts_with(MANAGED_LOG_PREFIX)
        && std::path::Path::new(file_name)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|extension| extension.eq_ignore_ascii_case(MANAGED_LOG_EXTENSION))
        && !file_name.contains(['/', '\\', ':'])
}
