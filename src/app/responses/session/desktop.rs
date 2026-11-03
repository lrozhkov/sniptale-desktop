//! Desktop authority, installation origin binding and heartbeat lease renewal.

use super::NativeHostSession;
#[cfg(any(windows, test))]
use super::{ControllerLeaseDecision, NativeControllerIdentity};

impl NativeHostSession {
    #[cfg(any(windows, test))]
    pub(in crate::app::responses) fn connection_was_replaced(&mut self, now_ms: u64) -> bool {
        if !self.connection_retired
            && let Some(controller) = self.active_controller.as_ref()
        {
            self.connection_retired = self
                .shared_state
                .lease_book
                .lock()
                .is_ok_and(|book| book.same_profile_owned_elsewhere(controller, now_ms));
        }
        self.connection_retired
    }

    #[cfg(any(windows, test))]
    pub(in crate::app::responses) fn observer_decision(
        &self,
        controller: &NativeControllerIdentity,
        now_ms: u64,
    ) -> Option<ControllerLeaseDecision> {
        self.shared_state
            .lease_book
            .lock()
            .ok()?
            .observer_decision(controller.clone(), now_ms)
            .ok()
    }

    #[cfg(any(windows, test))]
    pub(in crate::app::responses) fn renew_current_lease(
        &mut self,
        now_ms: u64,
    ) -> Option<ControllerLeaseDecision> {
        if self.connection_was_replaced(now_ms) {
            return None;
        }
        let controller = self.active_controller.as_ref()?;
        let lease_id = self.controller_lease_id.as_deref()?;
        if !self.validate_active_lease(lease_id, now_ms).is_valid() {
            return None;
        }
        self.shared_state
            .lease_book
            .lock()
            .ok()?
            .refresh(controller, lease_id, now_ms)
            .ok()
    }
    #[cfg(any(windows, test))]
    pub(crate) fn bind_authenticated_origin(&mut self, origin: &str) {
        self.authenticated_extension_id = origin
            .strip_prefix("chrome-extension://")
            .and_then(|value| value.strip_suffix('/'))
            .map(str::to_owned);
    }

    pub(in crate::app::responses) fn accepts_extension_id(&self, id: &str) -> bool {
        match self.authenticated_extension_id.as_deref() {
            Some(expected) => expected == id,
            None => !self.desktop_owned(),
        }
    }
    #[cfg(any(windows, test))]
    pub(in crate::app::responses) fn desktop_settings(
        &self,
    ) -> Option<&crate::settings::ActiveSettings> {
        self.shared_state.desktop_settings.as_ref()
    }

    pub(crate) fn desktop_owned(&self) -> bool {
        self.shared_state.desktop_authority
    }
}

#[cfg(test)]
mod tests {
    use super::NativeHostSession;
    use crate::{app::responses::NativeHostSharedState, settings::ActiveSettings};

    #[test]
    fn desktop_handshake_requires_a_matching_authenticated_origin() {
        let shared = NativeHostSharedState::with_desktop_settings(
            ActiveSettings::extension_managed_default(),
        );
        let mut session = NativeHostSession::with_shared_state(shared);
        let id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        assert!(!session.accepts_extension_id(id));
        session.bind_authenticated_origin("chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/");
        assert!(session.accepts_extension_id(id));
        assert!(!session.accepts_extension_id("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"));
        session.bind_authenticated_origin("invalid-origin");
        assert!(!session.accepts_extension_id(id));
    }
}
