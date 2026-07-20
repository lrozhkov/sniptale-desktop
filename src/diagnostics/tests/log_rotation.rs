use super::super::log::{
    LogFileSnapshot, LogRetentionPolicy, LogRotationError, default_log_retention_policy,
    plan_log_rotation,
};

const DAY_MS: u64 = 24 * 60 * 60 * 1_000;

#[test]
fn default_log_rotation_policy_matches_full_v1_limits() {
    let policy = default_log_retention_policy();

    assert_eq!(policy.max_file_bytes, 10 * 1024 * 1024);
    assert_eq!(policy.max_files, 10);
    assert_eq!(policy.retention_days, 14);
}

#[test]
fn rotation_plan_rotates_at_size_limit_and_keeps_newest_files() -> Result<(), LogRotationError> {
    let policy = LogRetentionPolicy {
        max_file_bytes: 10,
        max_files: 2,
        retention_days: 14,
    };
    let files = vec![
        log_file("sniptale-1.jsonl", 1_000, 4),
        log_file("sniptale-2.jsonl", 2_000, 4),
        log_file("sniptale-3.jsonl", 3_000, 4),
    ];

    let plan = plan_log_rotation(policy, 10, 3_500, &files)?;

    assert!(plan.rotate_active);
    assert_eq!(
        plan.keep_file_names,
        vec!["sniptale-3.jsonl", "sniptale-2.jsonl"]
    );
    assert_eq!(plan.delete_file_names, vec!["sniptale-1.jsonl"]);
    Ok(())
}

#[test]
fn rotation_plan_deletes_files_past_retention() -> Result<(), LogRotationError> {
    let policy = LogRetentionPolicy {
        max_file_bytes: 10,
        max_files: 10,
        retention_days: 14,
    };
    let now = 20 * DAY_MS;
    let files = vec![
        log_file("recent.jsonl", now - DAY_MS, 4),
        log_file("old.jsonl", now - (15 * DAY_MS), 4),
    ];

    let plan = plan_log_rotation(policy, 9, now, &files)?;

    assert!(!plan.rotate_active);
    assert_eq!(plan.keep_file_names, vec!["recent.jsonl"]);
    assert_eq!(plan.delete_file_names, vec!["old.jsonl"]);
    Ok(())
}

#[test]
fn rotation_plan_rejects_paths_and_invalid_policy() {
    let policy = default_log_retention_policy();
    let invalid_path = vec![log_file("../secret.jsonl", 1_000, 1)];

    assert_eq!(
        plan_log_rotation(policy, 0, 1_000, &invalid_path),
        Err(LogRotationError::FileName)
    );
    assert_eq!(
        plan_log_rotation(
            LogRetentionPolicy {
                max_file_bytes: 0,
                max_files: 10,
                retention_days: 14,
            },
            0,
            1_000,
            &[]
        ),
        Err(LogRotationError::MaxFileBytes)
    );
}

fn log_file(name: &str, created_at_epoch_ms: u64, size_bytes: u64) -> LogFileSnapshot {
    LogFileSnapshot {
        file_name: name.to_owned(),
        created_at_epoch_ms,
        size_bytes,
    }
}
