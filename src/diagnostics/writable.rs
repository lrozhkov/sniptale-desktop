//! Writable diagnostics directory probes.

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::log::setup::{LogSetupInputs, plan_process_logging};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WritableDirectoryStatus {
    Writable,
    Unwritable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WritableDirectoryFact {
    pub(crate) category: &'static str,
    pub(crate) status: WritableDirectoryStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WritableDirectoryReport {
    pub(crate) facts: Vec<WritableDirectoryFact>,
}

impl WritableDirectoryReport {
    pub(crate) fn all_writable(&self) -> bool {
        self.facts
            .iter()
            .all(|fact| fact.status == WritableDirectoryStatus::Writable)
    }

    pub(crate) fn failed_categories(&self) -> Vec<&'static str> {
        self.facts
            .iter()
            .filter(|fact| fact.status == WritableDirectoryStatus::Unwritable)
            .map(|fact| fact.category)
            .collect()
    }
}

pub(crate) fn runtime_writable_directory_report() -> WritableDirectoryReport {
    writable_directory_report(&WritableDirectoryInputs {
        local_app_data_dir: std::env::var_os("LOCALAPPDATA").map(PathBuf::from),
        temp_dir: std::env::temp_dir(),
        process_id: std::process::id(),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WritableDirectoryInputs {
    pub(crate) local_app_data_dir: Option<PathBuf>,
    pub(crate) temp_dir: PathBuf,
    pub(crate) process_id: u32,
}

pub(crate) fn writable_directory_report(
    inputs: &WritableDirectoryInputs,
) -> WritableDirectoryReport {
    let directories = runtime_directory_plans(inputs);
    WritableDirectoryReport {
        facts: directories
            .into_iter()
            .map(|directory| WritableDirectoryFact {
                category: directory.category,
                status: probe_writable_directory(&directory.path, inputs.process_id),
            })
            .collect(),
    }
}

pub(super) struct RuntimeDirectoryPlan {
    pub(super) category: &'static str,
    pub(super) path: PathBuf,
}

pub(super) fn runtime_directory_plans(
    inputs: &WritableDirectoryInputs,
) -> Vec<RuntimeDirectoryPlan> {
    let app_root = inputs
        .local_app_data_dir
        .clone()
        .unwrap_or_else(|| inputs.temp_dir.clone())
        .join("Sniptale")
        .join("NativeAgent");
    let log_inputs = LogSetupInputs {
        process_mode: "doctor-json",
        local_app_data_dir: inputs.local_app_data_dir.clone(),
        temp_dir: inputs.temp_dir.clone(),
        env_level: None,
        cli_level: None,
        cli_log_dir: None,
        debug_build: cfg!(debug_assertions),
    };
    let log_dir = plan_process_logging(&log_inputs)
        .map_or_else(|_| app_root.join("logs"), |plan| plan.directory);

    vec![
        RuntimeDirectoryPlan {
            category: "logs",
            path: log_dir,
        },
        RuntimeDirectoryPlan {
            category: "diagnostics",
            path: inputs
                .temp_dir
                .join("sniptale-native-agent")
                .join("diagnostics"),
        },
        RuntimeDirectoryPlan {
            category: "temp-media",
            path: app_root.join("temp-media"),
        },
        RuntimeDirectoryPlan {
            category: "cache",
            path: app_root,
        },
    ]
}

fn probe_writable_directory(path: &Path, process_id: u32) -> WritableDirectoryStatus {
    if fs::create_dir_all(path).is_err() {
        return WritableDirectoryStatus::Unwritable;
    }
    let probe = path.join(format!(".sniptale-write-test-{process_id}.tmp"));
    match fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = fs::remove_file(probe);
            WritableDirectoryStatus::Writable
        }
        Err(_) => WritableDirectoryStatus::Unwritable,
    }
}
