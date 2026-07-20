//! Validation of a manually selected local publication root.

use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ManualDestination {
    id: String,
    root: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ManualDestinationError {
    InvalidRoot,
    Unavailable,
}

impl ManualDestination {
    pub(crate) fn open(id: String, root: PathBuf) -> Result<Self, ManualDestinationError> {
        if id.is_empty()
            || id.len() > 128
            || root.as_os_str().is_empty()
            || root.to_string_lossy().starts_with("\\\\")
            || root.to_string_lossy().starts_with("//")
            || !root.is_absolute()
            || root
                .components()
                .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
        {
            return Err(ManualDestinationError::InvalidRoot);
        }
        let metadata =
            std::fs::symlink_metadata(&root).map_err(|_| ManualDestinationError::Unavailable)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() || has_symlink_component(&root) {
            return Err(ManualDestinationError::Unavailable);
        }
        Ok(Self { id, root })
    }

    #[must_use]
    pub(crate) fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}

fn has_symlink_component(root: &Path) -> bool {
    let mut current = PathBuf::new();
    for component in root.components() {
        current.push(component.as_os_str());
        let Ok(metadata) = std::fs::symlink_metadata(&current) else {
            return true;
        };
        if metadata.file_type().is_symlink() {
            return true;
        }
    }
    false
}
