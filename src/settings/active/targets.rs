//! Serialized access to the Desktop-owned browser destination registry.

use super::super::{ActiveSettings, BrowserTarget, BrowserTargetError, BrowserTargetRegistry};

impl ActiveSettings {
    pub(crate) fn activate_browser_delivery(
        &self,
        target_id: &str,
    ) -> Result<(), BrowserTargetError> {
        self.browser_targets
            .lock()
            .map_err(|_| BrowserTargetError::Io)?
            .activate_delivery(target_id)
    }
    pub(crate) fn with_browser_target_store(
        mut self,
        path: std::path::PathBuf,
    ) -> Result<Self, BrowserTargetError> {
        self.browser_targets =
            std::sync::Arc::new(std::sync::Mutex::new(BrowserTargetRegistry::load(path)?));
        Ok(self)
    }

    pub(crate) fn browser_targets(&self) -> Result<BrowserTargetRegistry, BrowserTargetError> {
        self.browser_targets
            .lock()
            .map(|registry| registry.clone())
            .map_err(|_| BrowserTargetError::Io)
    }

    pub(crate) fn register_browser_target(
        &self,
        controller: &crate::protocol::NativeControllerIdentity,
    ) -> Result<BrowserTarget, BrowserTargetError> {
        let target = BrowserTarget::from_controller(controller)?;
        self.browser_targets
            .lock()
            .map_err(|_| BrowserTargetError::Io)?
            .register(target.clone())?;
        Ok(target)
    }

    pub(crate) fn select_browser_target(
        &self,
        target_id: Option<&str>,
        ask_after_capture: bool,
    ) -> Result<(), BrowserTargetError> {
        self.browser_targets
            .lock()
            .map_err(|_| BrowserTargetError::Io)?
            .select(target_id, ask_after_capture)
    }
}
