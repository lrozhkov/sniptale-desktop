//! Native-host accepted hotkey settings state.

use crate::hotkeys::{HotkeyBinding, HotkeyError, HotkeyRegistrationPlan, HotkeyRegistry};

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct HotkeySettingsState {
    bindings: Vec<HotkeyBinding>,
    registry: HotkeyRegistry,
    latest_plan: HotkeyRegistrationPlan,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct HotkeySettingsUpdate {
    bindings: Vec<HotkeyBinding>,
    registry: HotkeyRegistry,
    registration_plan: HotkeyRegistrationPlan,
}

impl HotkeySettingsState {
    pub(super) fn prepare(
        &self,
        bindings: Vec<HotkeyBinding>,
    ) -> Result<HotkeySettingsUpdate, HotkeyError> {
        let mut registry = self.registry.clone();
        let registration_plan = registry.apply_bindings(bindings.clone())?;
        Ok(HotkeySettingsUpdate {
            bindings,
            registry,
            registration_plan,
        })
    }

    pub(super) fn commit(&mut self, update: HotkeySettingsUpdate) {
        self.bindings = update.bindings;
        self.registry = update.registry;
        self.latest_plan = update.registration_plan;
    }
}

impl HotkeySettingsUpdate {
    pub(super) const fn registration_plan(&self) -> &HotkeyRegistrationPlan {
        &self.registration_plan
    }

    pub(super) fn rollback_plan(&self) -> HotkeyRegistrationPlan {
        HotkeyRegistrationPlan {
            unregister_ids: self
                .registration_plan
                .register
                .iter()
                .map(|registered| registered.registration_id)
                .collect(),
            register: self.registration_plan.restore_on_failure.clone(),
            restore_on_failure: self.registration_plan.register.clone(),
        }
    }
}
