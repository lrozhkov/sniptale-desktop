//! Active lease fencing against the current Desktop destination choice.

use super::{ActiveLeaseValidation, NativeHostSession};
impl NativeHostSession {
    pub(in crate::app::responses) fn validate_active_lease(
        &self,
        controller_lease_id: &str,
        now_epoch_ms: u64,
    ) -> ActiveLeaseValidation {
        let Some(controller) = self.active_controller.as_ref() else {
            return ActiveLeaseValidation::Invalid;
        };
        #[cfg(any(windows, test))]
        if let Some(settings) = self.desktop_settings() {
            let Ok(target) = crate::settings::BrowserTarget::from_controller(controller) else {
                return ActiveLeaseValidation::Invalid;
            };
            if !settings
                .browser_targets()
                .is_ok_and(|registry| registry.selected_target_id() == Some(target.id.as_str()))
            {
                return ActiveLeaseValidation::Invalid;
            }
        }
        let Ok(lease_book) = self.shared_state.lease_book.lock() else {
            return ActiveLeaseValidation::StateUnavailable;
        };
        if lease_book
            .validate(controller, controller_lease_id, now_epoch_ms)
            .is_ok()
        {
            ActiveLeaseValidation::Valid
        } else {
            ActiveLeaseValidation::Invalid
        }
    }
}
