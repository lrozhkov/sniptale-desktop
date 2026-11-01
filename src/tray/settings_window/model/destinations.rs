//! Native presentation of trusted browser destinations and explicit delivery choice.

use super::SettingsWindowModel;

impl SettingsWindowModel {
    pub(crate) fn refresh_browser_targets(&mut self, active: &crate::settings::ActiveSettings) {
        if let Ok(registry) = active.browser_targets() {
            self.browser_targets = registry.targets().to_vec();
            self.browser_destination = registry.default_target_id().map(str::to_owned);
            self.ask_after_capture = registry.ask_after_capture();
        }
    }

    pub(crate) fn select_browser_destination(&mut self, index: usize) {
        if index == 0 {
            self.browser_destination = None;
        } else if let Some(target) = self.browser_targets.get(index - 1) {
            self.browser_destination = Some(target.id.clone());
        }
    }

    pub(crate) fn toggle_ask_after_capture(&mut self) {
        self.ask_after_capture = !self.ask_after_capture;
    }

    pub(crate) fn save_browser_destination(
        &mut self,
        active: &crate::settings::ActiveSettings,
    ) -> bool {
        if active
            .select_browser_target(self.browser_destination.as_deref(), self.ask_after_capture)
            .is_err()
        {
            self.feedback = Some(super::SettingsWindowFeedback::SaveFailed);
            false
        } else {
            self.refresh_browser_targets(active);
            true
        }
    }
}
