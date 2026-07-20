//! OS-backed Windows Program Files root discovery.

#![cfg(windows)]

use std::path::PathBuf;

use super::super::registration_facts::RegistryHive;

const PROGRAM_FILES_REGISTRY_SUBKEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion";
const PROGRAM_FILES_REGISTRY_VALUE_NAMES: &[&str] = &[
    "ProgramFilesDir",
    "ProgramFilesDir (x86)",
    "ProgramW6432Dir",
];

pub(crate) fn current_program_files_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for value_name in PROGRAM_FILES_REGISTRY_VALUE_NAMES {
        let Some(Ok(root)) = read_program_files_root(value_name) else {
            continue;
        };
        if root.is_empty() {
            continue;
        }
        let root = PathBuf::from(root);
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    roots
}

fn read_program_files_root(value_name: &str) -> Option<Result<String, ()>> {
    use super::registry::read_string_value;

    match read_string_value(
        RegistryHive::LocalMachine,
        PROGRAM_FILES_REGISTRY_SUBKEY,
        Some(value_name),
    ) {
        Ok(Some(value)) => Some(Ok(value)),
        Ok(None) => None,
        Err(_) => Some(Err(())),
    }
}
