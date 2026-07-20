//! Runtime application of validated hotkey registration plans.

use std::{error::Error, fmt};

use super::registry::{HotkeyRegistrationPlan, RegisteredHotkey};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum HotkeyRuntimeError {
    #[cfg(test)]
    RegisterFailed { registration_id: u32 },
    #[cfg(windows)]
    RegisterFailedWithOsError {
        registration_id: u32,
        os_error_code: u32,
    },
}

impl fmt::Display for HotkeyRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            #[cfg(test)]
            Self::RegisterFailed { registration_id } => {
                write!(
                    formatter,
                    "hotkey registration failed for id {registration_id}"
                )
            }
            #[cfg(windows)]
            Self::RegisterFailedWithOsError {
                registration_id,
                os_error_code,
            } => write!(
                formatter,
                "hotkey registration failed for id {registration_id} with OS error {os_error_code}"
            ),
        }
    }
}

impl Error for HotkeyRuntimeError {}

pub(crate) trait HotkeyRegistrar {
    type Handle;

    fn register(&mut self, hotkey: &RegisteredHotkey) -> Result<Self::Handle, HotkeyRuntimeError>;
}

pub(crate) trait HotkeyEventSource {
    fn drain_hotkey_events(&mut self) -> Vec<u32>;
}

#[derive(Debug)]
pub(crate) struct ActiveHotkeyRegistration<Handle> {
    registration: RegisteredHotkey,
    _handle: Handle,
}

#[derive(Debug)]
pub(crate) struct HotkeyRuntime<Handle> {
    active: Vec<ActiveHotkeyRegistration<Handle>>,
}

impl<Handle> Default for HotkeyRuntime<Handle> {
    fn default() -> Self {
        Self { active: Vec::new() }
    }
}

impl<Handle> HotkeyRuntime<Handle> {
    pub(crate) fn clear(&mut self) {
        self.active.clear();
    }

    #[must_use]
    pub(crate) fn active_registrations(&self) -> Vec<RegisteredHotkey> {
        self.active
            .iter()
            .map(|active| active.registration.clone())
            .collect()
    }

    #[must_use]
    pub(crate) fn action_for_registration_id(&self, registration_id: u32) -> Option<&str> {
        self.active
            .iter()
            .find(|active| active.registration.registration_id == registration_id)
            .map(|active| active.registration.action_id.as_str())
    }

    pub(crate) fn apply_plan<R>(
        &mut self,
        registrar: &mut R,
        plan: &HotkeyRegistrationPlan,
    ) -> Result<(), HotkeyRuntimeError>
    where
        R: HotkeyRegistrar<Handle = Handle>,
    {
        self.drop_unregistered(&plan.unregister_ids);
        let mut staged = Vec::with_capacity(plan.register.len());
        for registration in &plan.register {
            let handle = match registrar.register(registration) {
                Ok(handle) => handle,
                Err(error) => {
                    drop(staged);
                    self.restore_after_failure(registrar, &plan.restore_on_failure);
                    return Err(error);
                }
            };
            staged.push(ActiveHotkeyRegistration {
                registration: registration.clone(),
                _handle: handle,
            });
        }
        self.active.extend(staged);
        Ok(())
    }

    fn drop_unregistered(&mut self, unregister_ids: &[u32]) {
        self.active
            .retain(|active| !unregister_ids.contains(&active.registration.registration_id));
    }

    fn restore_after_failure<R>(&mut self, registrar: &mut R, registrations: &[RegisteredHotkey])
    where
        R: HotkeyRegistrar<Handle = Handle>,
    {
        for registration in registrations {
            if let Ok(handle) = registrar.register(registration) {
                self.active.push(ActiveHotkeyRegistration {
                    registration: registration.clone(),
                    _handle: handle,
                });
            }
        }
    }
}
