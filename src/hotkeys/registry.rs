//! Win32 `RegisterHotKey` lifecycle owner.

use super::dispatch::{HotkeyBinding, HotkeyChord, HotkeyDispatchMap, HotkeyError};

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum HotkeyPlanRoute {
    #[default]
    Extension,
    Manual,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RegisteredHotkey {
    pub(crate) registration_id: u32,
    pub(crate) hotkey: HotkeyChord,
    pub(crate) action_id: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct HotkeyRegistrationPlan {
    pub(crate) unregister_ids: Vec<u32>,
    pub(crate) register: Vec<RegisteredHotkey>,
    pub(crate) restore_on_failure: Vec<RegisteredHotkey>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HotkeyRegistry {
    registered: Vec<RegisteredHotkey>,
    next_registration_id: u32,
}

impl Default for HotkeyRegistry {
    fn default() -> Self {
        Self {
            registered: Vec::new(),
            next_registration_id: 1,
        }
    }
}

impl HotkeyRegistry {
    pub(crate) fn apply_bindings(
        &mut self,
        bindings: Vec<HotkeyBinding>,
    ) -> Result<HotkeyRegistrationPlan, HotkeyError> {
        HotkeyDispatchMap::new(bindings.clone())?;
        let desired = normalize_bindings(bindings);
        if same_registered_bindings(&self.registered, &desired) {
            return Ok(HotkeyRegistrationPlan {
                unregister_ids: Vec::new(),
                register: Vec::new(),
                restore_on_failure: Vec::new(),
            });
        }

        let unregister_ids = self
            .registered
            .iter()
            .map(|registered| registered.registration_id)
            .collect();
        let restore_on_failure = self.registered.clone();
        let register = self.allocate_registered(desired);
        self.registered.clone_from(&register);
        Ok(HotkeyRegistrationPlan {
            unregister_ids,
            register,
            restore_on_failure,
        })
    }
}

fn normalize_bindings(bindings: Vec<HotkeyBinding>) -> Vec<HotkeyBinding> {
    bindings
        .into_iter()
        .map(|binding| HotkeyBinding {
            hotkey: binding.hotkey.normalized(),
            action_id: binding.action_id,
        })
        .collect()
}

fn same_registered_bindings(registered: &[RegisteredHotkey], desired: &[HotkeyBinding]) -> bool {
    registered.len() == desired.len()
        && registered.iter().zip(desired).all(|(registered, desired)| {
            registered.hotkey == desired.hotkey && registered.action_id == desired.action_id
        })
}

impl HotkeyRegistry {
    fn allocate_registered(&mut self, desired: Vec<HotkeyBinding>) -> Vec<RegisteredHotkey> {
        desired
            .into_iter()
            .map(|binding| {
                let registration_id = self.next_registration_id;
                self.next_registration_id = self.next_registration_id.saturating_add(1);
                RegisteredHotkey {
                    registration_id,
                    hotkey: binding.hotkey,
                    action_id: binding.action_id,
                }
            })
            .collect()
    }
}
