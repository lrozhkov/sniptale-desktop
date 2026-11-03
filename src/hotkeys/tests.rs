use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

use super::registry::{HotkeyRegistrationPlan, HotkeyRegistry, RegisteredHotkey};
use super::runtime::{HotkeyRegistrar, HotkeyRuntime, HotkeyRuntimeError};
use super::{
    dispatch::{HotkeyBinding, HotkeyDispatchMap, HotkeyError, HotkeyKey, HotkeyModifiers},
    settings::{bindings_from_action_catalog, bindings_from_manual_hotkeys},
};
use crate::{
    protocol::{NativeTrayActionDescriptor, NativeTrayActionKind, NativeTrayActionRegistry},
    settings::{ActionCatalog, action_catalog_from_native_actions},
};

fn screenshot_binding() -> HotkeyBinding {
    HotkeyBinding {
        hotkey: HotkeyKey::Character('s').with_modifiers(HotkeyModifiers::CTRL),
        action_id: "capture-screen".to_owned(),
    }
}

#[test]
fn resolves_configured_hotkey_to_action() -> Result<(), HotkeyError> {
    let map = HotkeyDispatchMap::new(vec![screenshot_binding()])?;

    assert_eq!(
        map.resolve(HotkeyKey::Character('S').with_modifiers(HotkeyModifiers::CTRL)),
        Some("capture-screen")
    );
    assert_eq!(
        map.resolve(HotkeyKey::Character('s').with_modifiers(HotkeyModifiers::ALT)),
        None
    );
    Ok(())
}

#[test]
fn rejects_duplicate_hotkey_bindings() {
    let duplicate = HotkeyDispatchMap::new(vec![screenshot_binding(), screenshot_binding()]);

    assert!(matches!(duplicate, Err(HotkeyError::DuplicateHotkey)));
}

#[test]
fn rejects_empty_action_ids_and_modifierless_bindings() {
    assert!(matches!(
        HotkeyDispatchMap::new(vec![HotkeyBinding {
            hotkey: HotkeyKey::Character('s').with_modifiers(HotkeyModifiers::CTRL),
            action_id: String::new(),
        }]),
        Err(HotkeyError::EmptyActionId)
    ));
    assert!(matches!(
        HotkeyDispatchMap::new(vec![HotkeyBinding {
            hotkey: HotkeyKey::Character('s').with_modifiers(HotkeyModifiers::default()),
            action_id: "capture-screen".to_owned(),
        }]),
        Err(HotkeyError::MissingModifier)
    ));
}

#[test]
fn derives_hotkey_bindings_from_tray_shortcut_labels() -> Result<(), HotkeyError> {
    let catalog = action_catalog(&[
        ("capture-screen", "PrintScreen", true),
        ("start-recording", "Ctrl+Shift+R", true),
        ("stop-recording", "Ctrl+Shift+S", true),
        ("disabled-action", "Ctrl+Shift+D", false),
    ])?;

    let bindings = bindings_from_action_catalog(&catalog)?;

    assert_eq!(bindings.len(), 3);
    assert_eq!(bindings[0].hotkey.key(), HotkeyKey::PrintScreen);
    assert_eq!(bindings[0].action_id, "capture-screen");
    assert_eq!(
        bindings[1].hotkey,
        HotkeyKey::Character('R')
            .with_modifiers(HotkeyModifiers::CTRL.with(HotkeyModifiers::SHIFT))
    );
    assert_eq!(
        bindings[2].hotkey,
        HotkeyKey::Character('S')
            .with_modifiers(HotkeyModifiers::CTRL.with(HotkeyModifiers::SHIFT))
    );
    Ok(())
}

#[test]
fn rejects_unparseable_or_duplicate_tray_shortcut_labels() -> Result<(), HotkeyError> {
    let invalid = action_catalog(&[("capture-screen", "Ctrl++R", true)])?;
    assert_eq!(
        bindings_from_action_catalog(&invalid),
        Err(HotkeyError::InvalidShortcutLabel)
    );

    let duplicate = action_catalog(&[
        ("capture-screen", "Ctrl+Shift+S", true),
        ("stop-recording", "ctrl+shift+s", true),
    ])?;
    assert_eq!(
        bindings_from_action_catalog(&duplicate),
        Err(HotkeyError::DuplicateHotkey)
    );
    Ok(())
}

#[test]
fn derives_only_known_manual_hotkeys_and_rejects_duplicates() -> Result<(), HotkeyError> {
    let valid = BTreeMap::from([
        ("screenshot-screen".to_owned(), "Ctrl+Shift+1".to_owned()),
        ("recording-stop".to_owned(), "Ctrl+Shift+S".to_owned()),
    ]);
    let bindings = bindings_from_manual_hotkeys(&valid)?;
    assert_eq!(bindings.len(), 2);

    let e2e_fixture = BTreeMap::from([
        (
            "screenshot-screen".to_owned(),
            "Ctrl+Alt+Shift+F5".to_owned(),
        ),
        (
            "screenshot-region".to_owned(),
            "Ctrl+Alt+Shift+F6".to_owned(),
        ),
        (
            "recording-start-screen".to_owned(),
            "Ctrl+Alt+Shift+F7".to_owned(),
        ),
        ("recording-stop".to_owned(), "Ctrl+Alt+Shift+F8".to_owned()),
    ]);
    assert_eq!(bindings_from_manual_hotkeys(&e2e_fixture)?.len(), 4);

    let unknown = BTreeMap::from([("quit".to_owned(), "Ctrl+Shift+Q".to_owned())]);
    assert_eq!(
        bindings_from_manual_hotkeys(&unknown),
        Err(HotkeyError::InvalidShortcutLabel)
    );
    let duplicate = BTreeMap::from([
        ("screenshot-screen".to_owned(), "Ctrl+Shift+1".to_owned()),
        ("recording-stop".to_owned(), "Ctrl+Shift+1".to_owned()),
    ]);
    assert_eq!(
        bindings_from_manual_hotkeys(&duplicate),
        Err(HotkeyError::DuplicateHotkey)
    );
    Ok(())
}

#[test]
fn registry_plans_initial_registration_and_idempotent_reapply() -> Result<(), HotkeyError> {
    let mut registry = HotkeyRegistry::default();

    let plan = registry.apply_bindings(vec![screenshot_binding()])?;
    assert_eq!(plan.unregister_ids.len(), 0);
    assert_eq!(plan.register.len(), 1);
    assert_eq!(plan.register[0].registration_id, 1);
    assert_eq!(plan.register[0].action_id, "capture-screen");

    let plan = registry.apply_bindings(vec![screenshot_binding()])?;
    assert_eq!(plan.unregister_ids.len(), 0);
    assert_eq!(plan.register.len(), 0);
    Ok(())
}

#[test]
fn registry_rebinds_by_unregistering_previous_ids() -> Result<(), HotkeyError> {
    let mut registry = HotkeyRegistry::default();
    registry.apply_bindings(vec![screenshot_binding()])?;

    let plan = registry.apply_bindings(vec![HotkeyBinding {
        hotkey: HotkeyKey::Function(9)
            .with_modifiers(HotkeyModifiers::CTRL.with(HotkeyModifiers::SHIFT)),
        action_id: "capture-screen".to_owned(),
    }])?;

    assert_eq!(plan.unregister_ids, vec![1]);
    assert_eq!(plan.register[0].registration_id, 2);
    Ok(())
}

#[test]
fn registry_rejects_duplicate_bindings_before_registering() -> Result<(), HotkeyError> {
    let mut registry = HotkeyRegistry::default();
    registry.apply_bindings(vec![screenshot_binding()])?;

    let result = registry.apply_bindings(vec![screenshot_binding(), screenshot_binding()]);

    assert_eq!(result, Err(HotkeyError::DuplicateHotkey));
    let plan = registry.apply_bindings(vec![screenshot_binding()])?;
    assert_eq!(plan.register.len(), 0);
    Ok(())
}

#[test]
fn runtime_applies_registration_plan_and_drops_unregistered_handles()
-> Result<(), HotkeyRuntimeError> {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut registrar = FakeRegistrar::new(Rc::clone(&drops));
    let mut runtime = HotkeyRuntime::default();
    let mut registry = HotkeyRegistry::default();
    let initial = registry
        .apply_bindings(vec![screenshot_binding()])
        .map_err(|_| HotkeyRuntimeError::RegisterFailed { registration_id: 0 })?;
    runtime.apply_plan(&mut registrar, &initial)?;

    let replacement = registry
        .apply_bindings(vec![HotkeyBinding {
            hotkey: HotkeyKey::Function(9)
                .with_modifiers(HotkeyModifiers::CTRL.with(HotkeyModifiers::SHIFT)),
            action_id: "capture-screen".to_owned(),
        }])
        .map_err(|_| HotkeyRuntimeError::RegisterFailed { registration_id: 0 })?;
    runtime.apply_plan(&mut registrar, &replacement)?;

    assert_eq!(*drops.borrow(), vec![1]);
    assert_eq!(registrar.registered, vec![1, 2]);
    assert_eq!(runtime.active_registrations()[0].registration_id, 2);
    Ok(())
}

#[test]
fn runtime_rolls_back_partially_registered_new_handles_on_failure() {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut registrar = FakeRegistrar::new(Rc::clone(&drops)).with_failure(3);
    let mut runtime = HotkeyRuntime::default();
    let plan = HotkeyRegistrationPlan {
        unregister_ids: Vec::new(),
        register: vec![
            registered(2, HotkeyKey::Character('S'), "capture-screen"),
            registered(3, HotkeyKey::Character('R'), "record-screen"),
        ],
        restore_on_failure: Vec::new(),
    };

    let result = runtime.apply_plan(&mut registrar, &plan);

    assert_eq!(
        result,
        Err(HotkeyRuntimeError::RegisterFailed { registration_id: 3 })
    );
    assert_eq!(*drops.borrow(), vec![2]);
    assert_eq!(runtime.active_registrations().len(), 0);
}

#[test]
fn runtime_restores_previous_handles_when_replacement_registration_fails()
-> Result<(), HotkeyRuntimeError> {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut registrar = FakeRegistrar::new(Rc::clone(&drops));
    let mut runtime = HotkeyRuntime::default();
    let mut registry = HotkeyRegistry::default();
    let initial = registry
        .apply_bindings(vec![screenshot_binding()])
        .map_err(|_| HotkeyRuntimeError::RegisterFailed { registration_id: 0 })?;
    runtime.apply_plan(&mut registrar, &initial)?;
    registrar.fail_registration_id = Some(2);

    let replacement = registry
        .apply_bindings(vec![HotkeyBinding {
            hotkey: HotkeyKey::Function(9)
                .with_modifiers(HotkeyModifiers::CTRL.with(HotkeyModifiers::SHIFT)),
            action_id: "capture-screen".to_owned(),
        }])
        .map_err(|_| HotkeyRuntimeError::RegisterFailed { registration_id: 0 })?;
    let result = runtime.apply_plan(&mut registrar, &replacement);

    assert_eq!(
        result,
        Err(HotkeyRuntimeError::RegisterFailed { registration_id: 2 })
    );
    assert_eq!(*drops.borrow(), vec![1]);
    assert_eq!(registrar.registered, vec![1, 1]);
    assert_eq!(runtime.active_registrations()[0].registration_id, 1);
    Ok(())
}

fn registered(registration_id: u32, key: HotkeyKey, action_id: &str) -> RegisteredHotkey {
    RegisteredHotkey {
        registration_id,
        hotkey: key.with_modifiers(HotkeyModifiers::CTRL),
        action_id: action_id.to_owned(),
    }
}

fn action_catalog(actions: &[(&str, &str, bool)]) -> Result<ActionCatalog, HotkeyError> {
    let registry = NativeTrayActionRegistry {
        actions: actions
            .iter()
            .map(|(id, shortcut_label, enabled)| NativeTrayActionDescriptor {
                enabled: *enabled,
                id: (*id).to_owned(),
                kind: NativeTrayActionKind::OpenGallery,
                label: (*id).to_owned(),
                shortcut_label: Some((*shortcut_label).to_owned()),
                warning: None,
                offline_capable: Some(false),
            })
            .collect(),
        revision: "tray-revision-1".to_owned(),
        shortcut_priority: None,
    };
    action_catalog_from_native_actions(&registry).map_err(|_| HotkeyError::InvalidShortcutLabel)
}

#[derive(Debug)]
struct FakeHandle {
    registration_id: u32,
    drops: Rc<RefCell<Vec<u32>>>,
}

impl Drop for FakeHandle {
    fn drop(&mut self) {
        self.drops.borrow_mut().push(self.registration_id);
    }
}

#[derive(Debug)]
struct FakeRegistrar {
    drops: Rc<RefCell<Vec<u32>>>,
    fail_registration_id: Option<u32>,
    registered: Vec<u32>,
}

impl FakeRegistrar {
    fn new(drops: Rc<RefCell<Vec<u32>>>) -> Self {
        Self {
            drops,
            fail_registration_id: None,
            registered: Vec::new(),
        }
    }

    const fn with_failure(mut self, registration_id: u32) -> Self {
        self.fail_registration_id = Some(registration_id);
        self
    }
}

impl HotkeyRegistrar for FakeRegistrar {
    type Handle = FakeHandle;

    fn register(&mut self, hotkey: &RegisteredHotkey) -> Result<Self::Handle, HotkeyRuntimeError> {
        if self.fail_registration_id == Some(hotkey.registration_id) {
            return Err(HotkeyRuntimeError::RegisterFailed {
                registration_id: hotkey.registration_id,
            });
        }
        self.registered.push(hotkey.registration_id);
        Ok(FakeHandle {
            registration_id: hotkey.registration_id,
            drops: Rc::clone(&self.drops),
        })
    }
}
