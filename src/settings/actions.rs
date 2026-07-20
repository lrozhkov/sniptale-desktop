//! Extension-owned action catalog accepted by native settings sync.

use std::collections::HashSet;

use crate::protocol::{ExtensionIntentActionId, NativeTrayActionKind, NativeTrayActionRegistry};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ActionCatalogError {
    EmptyRevision,
    EmptyActionId,
    EmptyLabel,
    DuplicateActionId,
    ReservedShellActionId,
    InvalidExtensionIntent,
    ExtensionIntentKindMismatch,
    ExtensionIntentOfflineCapable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ActionCatalog {
    entries: Vec<ActionCatalogEntry>,
}

impl ActionCatalog {
    #[must_use]
    pub(crate) fn entries(&self) -> &[ActionCatalogEntry] {
        &self.entries
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ActionCatalogEntry {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(crate) kind: NativeTrayActionKind,
    pub(crate) extension_intent: Option<ExtensionIntentActionId>,
    pub(crate) enabled: bool,
    pub(crate) offline_capable: bool,
    pub(crate) shortcut_label: Option<String>,
    pub(crate) warning: Option<String>,
}

pub(crate) fn action_catalog_from_native_actions(
    registry: &NativeTrayActionRegistry,
) -> Result<ActionCatalog, ActionCatalogError> {
    if registry.revision.is_empty() {
        return Err(ActionCatalogError::EmptyRevision);
    }
    let mut seen = HashSet::with_capacity(registry.actions.len());
    let mut entries = Vec::with_capacity(registry.actions.len());
    for descriptor in &registry.actions {
        if descriptor.id.is_empty() {
            return Err(ActionCatalogError::EmptyActionId);
        }
        if descriptor.label.is_empty() {
            return Err(ActionCatalogError::EmptyLabel);
        }
        if !seen.insert(descriptor.id.as_str()) {
            return Err(ActionCatalogError::DuplicateActionId);
        }
        if is_ignored_legacy_shell_descriptor(&descriptor.id, &descriptor.kind) {
            continue;
        }
        if is_shell_action_id(&descriptor.id) {
            return Err(ActionCatalogError::ReservedShellActionId);
        }
        let offline_capable = descriptor.offline_capable.unwrap_or(false);
        let canonical_intent = ExtensionIntentActionId::parse(&descriptor.id);
        let extension_intent =
            ExtensionIntentActionId::from_descriptor(&descriptor.id, &descriptor.kind);
        match descriptor.kind {
            NativeTrayActionKind::CaptureScreenshot | NativeTrayActionKind::StartRecording => {
                let intent = extension_intent.ok_or_else(|| {
                    if canonical_intent.is_some() {
                        ActionCatalogError::ExtensionIntentKindMismatch
                    } else {
                        ActionCatalogError::InvalidExtensionIntent
                    }
                })?;
                if !intent.matches_kind(&descriptor.kind) {
                    return Err(ActionCatalogError::ExtensionIntentKindMismatch);
                }
                if offline_capable {
                    return Err(ActionCatalogError::ExtensionIntentOfflineCapable);
                }
            }
            _ if canonical_intent.is_some() => {
                return Err(ActionCatalogError::ExtensionIntentKindMismatch);
            }
            _ => {}
        }
        entries.push(ActionCatalogEntry {
            id: descriptor.id.clone(),
            label: descriptor.label.clone(),
            kind: descriptor.kind.clone(),
            extension_intent,
            enabled: descriptor.enabled,
            offline_capable,
            shortcut_label: descriptor.shortcut_label.clone(),
            warning: descriptor.warning.clone(),
        });
    }
    Ok(ActionCatalog { entries })
}

fn is_ignored_legacy_shell_descriptor(id: &str, kind: &NativeTrayActionKind) -> bool {
    id == "open-settings" && *kind == NativeTrayActionKind::OpenSettings
}

fn is_shell_action_id(id: &str) -> bool {
    matches!(
        id,
        "recording-pause"
            | "recording-resume"
            | "recording-stop"
            | "recording-cancel"
            | "open-settings"
            | "mode-extension-managed"
            | "mode-manual"
            | "open-save-folder"
            | "open-extension-settings"
            | "retry-pending-saves"
            | "discard-pending-saves"
            | "reconnect"
            | "quit"
    )
}
