//! Desktop-owned browser destinations, explicit default selection, and bounded persistence.

pub(super) mod startup;
mod store;
#[cfg(test)]
mod tests;

use crate::protocol::{BrowserFamily, NativeControllerIdentity};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::PathBuf};

const MAX_TARGETS: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BrowserTargetError {
    Io,
    Invalid,
    Limit,
    Stale,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BrowserTarget {
    pub(crate) id: String,
    pub(crate) extension_id: String,
    pub(crate) browser_family: BrowserFamily,
    pub(crate) profile_key: String,
}

impl BrowserTarget {
    pub(crate) fn from_controller(
        controller: &NativeControllerIdentity,
    ) -> Result<Self, BrowserTargetError> {
        if !valid_id(&controller.extension_id) || !valid_id(&controller.profile_key) {
            return Err(BrowserTargetError::Invalid);
        }
        let family = match controller.browser_family {
            BrowserFamily::Chrome => "chrome",
            BrowserFamily::Edge => "edge",
            BrowserFamily::Chromium => "chromium",
            BrowserFamily::Unknown => "unknown",
        };
        let mut digest = Sha256::new();
        for component in [
            "sniptale-browser-target-v1",
            &controller.extension_id,
            family,
            &controller.profile_key,
        ] {
            digest.update(component.as_bytes());
            digest.update(b"\0");
        }
        Ok(Self {
            id: format!("{:x}", digest.finalize()),
            extension_id: controller.extension_id.clone(),
            browser_family: controller.browser_family.clone(),
            profile_key: controller.profile_key.clone(),
        })
    }

    pub(crate) fn label(&self) -> String {
        let family = match self.browser_family {
            BrowserFamily::Chrome => "Chrome",
            BrowserFamily::Edge => "Edge",
            BrowserFamily::Chromium => "Chromium",
            BrowserFamily::Unknown => "Browser",
        };
        format!(
            "{family} — {}",
            self.profile_key.chars().take(24).collect::<String>()
        )
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct BrowserTargetState {
    schema_version: u16,
    targets: Vec<BrowserTarget>,
    default_target_id: Option<String>,
    ask_after_capture: bool,
    #[serde(default)]
    delivery_target_id: Option<String>,
}

impl Default for BrowserTargetState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            targets: Vec::new(),
            default_target_id: None,
            ask_after_capture: false,
            delivery_target_id: None,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct BrowserTargetRegistry {
    state: BrowserTargetState,
    path: Option<PathBuf>,
}

impl BrowserTargetRegistry {
    pub(crate) fn load(path: PathBuf) -> Result<Self, BrowserTargetError> {
        Ok(Self {
            state: store::load(&path)?,
            path: Some(path),
        })
    }
    pub(crate) fn targets(&self) -> &[BrowserTarget] {
        &self.state.targets
    }
    pub(crate) fn default_target_id(&self) -> Option<&str> {
        self.state.default_target_id.as_deref()
    }
    pub(crate) fn selected_target_id(&self) -> Option<&str> {
        self.state
            .delivery_target_id
            .as_deref()
            .or(self.default_target_id())
    }
    pub(crate) fn activate_delivery(&mut self, target_id: &str) -> Result<(), BrowserTargetError> {
        if !self
            .state
            .targets
            .iter()
            .any(|target| target.id == target_id)
        {
            return Err(BrowserTargetError::Stale);
        }
        let mut candidate = self.state.clone();
        candidate.delivery_target_id = Some(target_id.to_owned());
        self.commit(candidate)
    }
    pub(crate) const fn ask_after_capture(&self) -> bool {
        self.state.ask_after_capture
    }

    pub(crate) fn register(&mut self, target: BrowserTarget) -> Result<(), BrowserTargetError> {
        if self.state.targets.contains(&target) {
            return Ok(());
        }
        if self.state.targets.len() >= MAX_TARGETS {
            return Err(BrowserTargetError::Limit);
        }
        let mut candidate = self.state.clone();
        candidate.targets.push(target);
        self.commit(candidate)
    }

    pub(crate) fn select(
        &mut self,
        target_id: Option<&str>,
        ask_after_capture: bool,
    ) -> Result<(), BrowserTargetError> {
        if target_id.is_some_and(|id| !self.state.targets.iter().any(|target| target.id == id)) {
            return Err(BrowserTargetError::Stale);
        }
        let mut candidate = self.state.clone();
        candidate.default_target_id = target_id.map(str::to_owned);
        candidate.ask_after_capture = ask_after_capture;
        candidate.delivery_target_id = None;
        self.commit(candidate)
    }

    fn commit(&mut self, candidate: BrowserTargetState) -> Result<(), BrowserTargetError> {
        validate(&candidate)?;
        if let Some(path) = &self.path {
            store::save(path, &candidate)?;
        }
        self.state = candidate;
        Ok(())
    }
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn validate(state: &BrowserTargetState) -> Result<(), BrowserTargetError> {
    if state.schema_version != 1 || state.targets.len() > MAX_TARGETS {
        return Err(BrowserTargetError::Invalid);
    }
    let mut ids = BTreeSet::new();
    for target in &state.targets {
        let expected = BrowserTarget::from_controller(&NativeControllerIdentity {
            extension_id: target.extension_id.clone(),
            browser_family: target.browser_family.clone(),
            profile_key: target.profile_key.clone(),
            connection_id: "validation".to_owned(),
            document_id: None,
        })?;
        if target != &expected || !ids.insert(&target.id) {
            return Err(BrowserTargetError::Invalid);
        }
    }
    if state
        .default_target_id
        .as_ref()
        .is_some_and(|id| !ids.contains(id))
    {
        return Err(BrowserTargetError::Invalid);
    }
    if state
        .delivery_target_id
        .as_ref()
        .is_some_and(|id| !ids.contains(id))
    {
        return Err(BrowserTargetError::Invalid);
    }
    Ok(())
}
