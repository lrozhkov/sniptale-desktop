use std::{error::Error, fs, io::Write};

use super::super::log::{
    LogRetentionPolicy,
    file::{LogFileLifecycleError, prepare_managed_log_file_with_policy},
};

#[test]
fn managed_log_file_lifecycle_creates_active_file_and_keeps_unmanaged_files()
-> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    fs::write(temp.path().join("notes.txt"), "leave alone")?;

    let mut prepared = prepare_managed_log_file_with_policy(
        temp.path(),
        1_000,
        42,
        LogRetentionPolicy {
            max_file_bytes: 10,
            max_files: 10,
            retention_days: 14,
        },
    )?;
    prepared.file.write_all(b"{\"message\":\"ok\"}\n")?;

    assert_eq!(
        prepared.outcome.active_file_name,
        "sniptale-native-1000-42-0.jsonl"
    );
    assert!(
        temp.path()
            .join(&prepared.outcome.active_file_name)
            .is_file()
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("notes.txt"))?,
        "leave alone"
    );
    Ok(())
}

#[test]
fn managed_log_file_lifecycle_uses_suffix_when_name_exists() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    fs::write(
        temp.path().join("sniptale-native-1000-42-0.jsonl"),
        "existing",
    )?;

    let prepared = prepare_managed_log_file_with_policy(
        temp.path(),
        1_000,
        42,
        LogRetentionPolicy {
            max_file_bytes: 10,
            max_files: 10,
            retention_days: 14,
        },
    )?;

    assert_eq!(
        prepared.outcome.active_file_name,
        "sniptale-native-1000-42-1.jsonl"
    );
    assert_eq!(
        fs::read_to_string(temp.path().join("sniptale-native-1000-42-0.jsonl"))?,
        "existing"
    );
    Ok(())
}

#[test]
fn managed_log_file_lifecycle_deletes_only_managed_logs_over_retention_limit()
-> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;
    for index in 0..4 {
        fs::write(
            temp.path()
                .join(format!("sniptale-native-1000-42-{index}.jsonl")),
            "old",
        )?;
    }
    fs::write(temp.path().join("sniptale-native-note.txt"), "unmanaged")?;

    let prepared = prepare_managed_log_file_with_policy(
        temp.path(),
        2_000,
        42,
        LogRetentionPolicy {
            max_file_bytes: 10,
            max_files: 1,
            retention_days: 14,
        },
    )?;

    let managed_count = fs::read_dir(temp.path())?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("sniptale-native-")
                && entry.file_name().to_string_lossy().ends_with(".jsonl")
        })
        .count();
    assert_eq!(managed_count, 2);
    assert_eq!(prepared.outcome.deleted_file_names.len(), 3);
    assert_eq!(
        fs::read_to_string(temp.path().join("sniptale-native-note.txt"))?,
        "unmanaged"
    );
    Ok(())
}

#[test]
fn managed_log_file_lifecycle_reports_invalid_policy() -> Result<(), Box<dyn Error>> {
    let temp = tempfile::tempdir()?;

    let result = prepare_managed_log_file_with_policy(
        temp.path(),
        1_000,
        42,
        LogRetentionPolicy {
            max_file_bytes: 0,
            max_files: 10,
            retention_days: 14,
        },
    );

    assert_eq!(
        result.map(|prepared| prepared.outcome),
        Err(LogFileLifecycleError::Rotation(
            super::super::log::LogRotationError::MaxFileBytes
        ))
    );
    Ok(())
}
