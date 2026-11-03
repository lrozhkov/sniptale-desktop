use std::{cell::RefCell, rc::Rc};

use crate::{
    app::resident::{AgentOutboundEvent, WorkerCommand},
    hotkeys::{
        HotkeyError, HotkeyEventSource, HotkeyRegistrar, HotkeyRegistry, HotkeyRuntimeError,
        RegisteredHotkey, bindings_from_action_catalog,
    },
    protocol::{NativeTrayActionDescriptor, NativeTrayActionKind, NativeTrayActionRegistry},
    settings::action_catalog_from_native_actions,
    tray::TrayNoticeKind,
};

use super::{HotkeyWorker, HotkeyWorkerControl};

#[test]
fn applies_hotkey_registration_plan_and_stops() -> Result<(), HotkeyError> {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut registrar = FakeRegistrar::new(Rc::clone(&drops));
    let mut worker = HotkeyWorker::default();
    let mut registry = HotkeyRegistry::default();
    let plan =
        registry.apply_bindings(bindings_for(&[("capture-screen", "Ctrl+Shift+S", true)])?)?;
    let (result_sender, result_receiver) = std::sync::mpsc::sync_channel(1);

    let result = worker.handle_command(
        &mut registrar,
        WorkerCommand::ApplyHotkeyPlan {
            plan,
            route: crate::hotkeys::HotkeyPlanRoute::Extension,
            result_sender,
        },
        None,
    );
    let stop = worker.handle_command(&mut registrar, WorkerCommand::Stop, None);

    assert_eq!(result, Ok(HotkeyWorkerControl::Continue));
    assert_eq!(
        result_receiver
            .recv()
            .map_err(|_| HotkeyError::InvalidShortcutLabel)?,
        Ok(())
    );
    assert_eq!(stop, Ok(HotkeyWorkerControl::Stop));
    assert_eq!(registrar.registered, vec![1]);
    assert_eq!(worker.active_registration_ids(), vec![1]);
    Ok(())
}

#[test]
fn keeps_previous_hotkeys_when_replacement_plan_fails() -> Result<(), HotkeyError> {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut registrar = FakeRegistrar::new(Rc::clone(&drops));
    let mut worker = HotkeyWorker::default();
    let mut registry = HotkeyRegistry::default();
    let initial =
        registry.apply_bindings(bindings_for(&[("capture-screen", "Ctrl+Shift+S", true)])?)?;
    let (result_sender, result_receiver) = std::sync::mpsc::sync_channel(1);
    worker
        .handle_command(
            &mut registrar,
            WorkerCommand::ApplyHotkeyPlan {
                plan: initial,
                route: crate::hotkeys::HotkeyPlanRoute::Extension,
                result_sender,
            },
            None,
        )
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?;
    assert_eq!(
        result_receiver
            .recv()
            .map_err(|_| HotkeyError::InvalidShortcutLabel)?,
        Ok(())
    );
    registrar.fail_registration_id = Some(2);

    let replacement =
        registry.apply_bindings(bindings_for(&[("capture-screen", "Ctrl+Shift+R", true)])?)?;
    let (result_sender, result_receiver) = std::sync::mpsc::sync_channel(1);
    let result = worker.handle_command(
        &mut registrar,
        WorkerCommand::ApplyHotkeyPlan {
            plan: replacement,
            route: crate::hotkeys::HotkeyPlanRoute::Extension,
            result_sender,
        },
        None,
    );

    assert_eq!(
        result,
        Err(HotkeyRuntimeError::RegisterFailed { registration_id: 2 })
    );
    assert_eq!(
        result_receiver
            .recv()
            .map_err(|_| HotkeyError::InvalidShortcutLabel)?,
        Err(HotkeyRuntimeError::RegisterFailed { registration_id: 2 })
    );
    assert_eq!(*drops.borrow(), vec![1]);
    assert_eq!(worker.active_registration_ids(), vec![1]);
    Ok(())
}

#[test]
fn mode_fence_snapshot_restores_the_previous_hotkey_projection() -> Result<(), HotkeyError> {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut registrar = FakeRegistrar::new(drops);
    let mut worker = registered_worker(&mut registrar, "capture-screen")?;
    let (fence_sender, fence_receiver) = std::sync::mpsc::sync_channel(1);
    worker
        .handle_command(
            &mut registrar,
            WorkerCommand::FenceMode {
                result_sender: fence_sender,
            },
            None,
        )
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?;
    assert_eq!(fence_receiver.recv().ok(), Some(false));
    assert_eq!(worker.active_registration_ids().len(), 0);

    let (restore_sender, restore_receiver) = std::sync::mpsc::sync_channel(1);
    worker
        .handle_command(
            &mut registrar,
            WorkerCommand::RestoreHotkeys {
                result_sender: restore_sender,
            },
            None,
        )
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?;

    assert_eq!(restore_receiver.recv().ok(), Some(Ok(())));
    assert_eq!(worker.active_registration_ids(), vec![1]);
    Ok(())
}

#[test]
fn rejected_manual_stage_restores_fenced_bindings_and_route() -> Result<(), HotkeyError> {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut registrar = FakeRegistrar::new(drops);
    let mut worker = registered_worker(&mut registrar, "capture-screen")?;
    let (fence_sender, fence_receiver) = std::sync::mpsc::sync_channel(1);
    worker
        .handle_command(
            &mut registrar,
            WorkerCommand::FenceMode {
                result_sender: fence_sender,
            },
            None,
        )
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?;
    assert_eq!(fence_receiver.recv().ok(), Some(false));

    registrar.fail_registration_id = Some(1);
    let mut registry = HotkeyRegistry::default();
    let staged = registry.apply_bindings(crate::hotkeys::bindings_from_manual_hotkeys(
        &std::collections::BTreeMap::from([(
            "screenshot-screen".to_owned(),
            "Ctrl+Shift+R".to_owned(),
        )]),
    )?)?;
    let (apply_sender, apply_receiver) = std::sync::mpsc::sync_channel(1);
    assert!(
        worker
            .handle_command(
                &mut registrar,
                WorkerCommand::ApplyHotkeyPlan {
                    plan: staged,
                    route: crate::hotkeys::HotkeyPlanRoute::Manual,
                    result_sender: apply_sender,
                },
                None,
            )
            .is_err()
    );
    assert!(matches!(
        apply_receiver.recv(),
        Ok(Err(HotkeyRuntimeError::RegisterFailed { .. }))
    ));

    registrar.fail_registration_id = None;
    let (restore_sender, restore_receiver) = std::sync::mpsc::sync_channel(1);
    worker
        .handle_command(
            &mut registrar,
            WorkerCommand::RestoreHotkeys {
                result_sender: restore_sender,
            },
            None,
        )
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?;

    assert_eq!(restore_receiver.recv().ok(), Some(Ok(())));
    assert_eq!(worker.active_registration_ids(), vec![1]);
    assert_eq!(
        worker.active_route(),
        crate::hotkeys::HotkeyPlanRoute::Extension
    );
    Ok(())
}

#[test]
fn persistence_rollback_removes_staged_hotkeys_and_keeps_snapshot_until_restore_succeeds()
-> Result<(), HotkeyError> {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut registrar = FakeRegistrar::new(Rc::clone(&drops));
    let mut worker = registered_worker(&mut registrar, "capture-screen")?;
    let (fence_sender, fence_receiver) = std::sync::mpsc::sync_channel(1);
    worker
        .handle_command(
            &mut registrar,
            WorkerCommand::FenceMode {
                result_sender: fence_sender,
            },
            None,
        )
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?;
    assert_eq!(fence_receiver.recv().ok(), Some(false));

    let mut registry = HotkeyRegistry::default();
    let staged = registry.apply_bindings(crate::hotkeys::bindings_from_manual_hotkeys(
        &std::collections::BTreeMap::from([(
            "screenshot-screen".to_owned(),
            "Ctrl+Shift+R".to_owned(),
        )]),
    )?)?;
    let (apply_sender, apply_receiver) = std::sync::mpsc::sync_channel(1);
    worker
        .handle_command(
            &mut registrar,
            WorkerCommand::ApplyHotkeyPlan {
                plan: staged,
                route: crate::hotkeys::HotkeyPlanRoute::Manual,
                result_sender: apply_sender,
            },
            None,
        )
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?;
    assert_eq!(apply_receiver.recv().ok(), Some(Ok(())));
    assert_eq!(worker.active_action_ids(), vec!["screenshot-screen"]);

    registrar.fail_registration_id = Some(1);
    let (failed_sender, failed_receiver) = std::sync::mpsc::sync_channel(1);
    assert!(
        worker
            .handle_command(
                &mut registrar,
                WorkerCommand::RestoreHotkeys {
                    result_sender: failed_sender,
                },
                None,
            )
            .is_err()
    );
    assert!(matches!(
        failed_receiver.recv(),
        Ok(Err(HotkeyRuntimeError::RegisterFailed { .. }))
    ));
    assert_eq!(worker.active_registration_ids().len(), 0);
    assert!(worker.has_fenced_snapshot());
    assert!(drops.borrow().len() >= 2);

    registrar.fail_registration_id = None;
    let (restore_sender, restore_receiver) = std::sync::mpsc::sync_channel(1);
    worker
        .handle_command(
            &mut registrar,
            WorkerCommand::RestoreHotkeys {
                result_sender: restore_sender,
            },
            None,
        )
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?;
    assert_eq!(restore_receiver.recv().ok(), Some(Ok(())));
    assert_eq!(worker.active_action_ids(), vec!["capture-screen"]);
    assert_eq!(
        worker.active_route(),
        crate::hotkeys::HotkeyPlanRoute::Extension
    );
    assert!(!worker.has_fenced_snapshot());
    Ok(())
}

#[test]
fn dispatches_registered_hotkey_events_to_agent_actions() -> Result<(), HotkeyError> {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut registrar = FakeRegistrar::new(Rc::clone(&drops));
    let mut worker = HotkeyWorker::default();
    let mut registry = HotkeyRegistry::default();
    let plan =
        registry.apply_bindings(bindings_for(&[("capture-screen", "Ctrl+Shift+S", true)])?)?;
    let (result_sender, result_receiver) = std::sync::mpsc::sync_channel(1);
    worker
        .handle_command(
            &mut registrar,
            WorkerCommand::ApplyHotkeyPlan {
                plan,
                route: crate::hotkeys::HotkeyPlanRoute::Extension,
                result_sender,
            },
            None,
        )
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?;
    assert_eq!(
        result_receiver
            .recv()
            .map_err(|_| HotkeyError::InvalidShortcutLabel)?,
        Ok(())
    );
    registrar.pending_events = vec![99, 1];
    let events = crate::app::resident::AgentEventHub::default();
    let receiver = events.subscribe();

    worker.drain_hotkey_events(&mut registrar, &events, None, None);

    let event = receiver
        .try_recv()
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?;
    assert!(matches!(
        event,
        AgentOutboundEvent::TrayActionRequested {
            ref action_id,
            ..
        } if action_id == "capture-screen"
    ));
    assert!(receiver.try_recv().is_err());
    Ok(())
}

#[test]
fn hotkey_event_notifies_tray_when_extension_session_is_absent() -> Result<(), HotkeyError> {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut registrar = FakeRegistrar::new(drops);
    let worker = registered_worker(&mut registrar, "capture-screen")?;
    registrar.pending_events = vec![1];
    let events = crate::app::resident::AgentEventHub::default();
    let (tray_sender, tray_receiver) = std::sync::mpsc::sync_channel(1);

    worker.drain_hotkey_events(&mut registrar, &events, Some(&tray_sender), None);

    assert!(matches!(
        tray_receiver.try_recv(),
        Ok(WorkerCommand::ShowTrayNotice {
            kind: TrayNoticeKind::ExtensionDisconnected
        })
    ));
    Ok(())
}

#[test]
fn manual_hotkey_event_is_routed_to_the_local_tray_worker() -> Result<(), HotkeyError> {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut registrar = FakeRegistrar::new(drops);
    let mut worker = HotkeyWorker::default();
    let mut registry = HotkeyRegistry::default();
    let plan = registry.apply_bindings(crate::hotkeys::bindings_from_manual_hotkeys(
        &std::collections::BTreeMap::from([(
            "screenshot-screen".to_owned(),
            "Ctrl+Shift+1".to_owned(),
        )]),
    )?)?;
    let (result_sender, result_receiver) = std::sync::mpsc::sync_channel(1);
    worker
        .handle_command(
            &mut registrar,
            WorkerCommand::ApplyHotkeyPlan {
                plan,
                route: crate::hotkeys::HotkeyPlanRoute::Manual,
                result_sender,
            },
            None,
        )
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?;
    result_receiver
        .recv()
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?;
    registrar.pending_events = vec![1];
    let events = crate::app::resident::AgentEventHub::default();
    let (tray_sender, tray_receiver) = std::sync::mpsc::sync_channel(1);

    worker.drain_hotkey_events(&mut registrar, &events, Some(&tray_sender), None);

    assert!(matches!(
        tray_receiver.try_recv(),
        Ok(WorkerCommand::ManualHotkeyAction { action_id }) if action_id == "screenshot-screen"
    ));
    Ok(())
}

#[test]
fn manual_hotkey_handoff_waits_for_a_saturated_tray_queue() -> Result<(), HotkeyError> {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut registrar = FakeRegistrar::new(drops);
    let mut worker = HotkeyWorker::default();
    let mut registry = HotkeyRegistry::default();
    let bindings =
        crate::hotkeys::bindings_from_manual_hotkeys(&std::collections::BTreeMap::from([(
            "screenshot-screen".to_owned(),
            "Ctrl+Shift+1".to_owned(),
        )]))?;
    let plan = registry.apply_bindings(bindings)?;
    let (result_sender, result_receiver) = std::sync::mpsc::sync_channel(1);
    worker
        .handle_command(
            &mut registrar,
            WorkerCommand::ApplyHotkeyPlan {
                plan,
                route: crate::hotkeys::HotkeyPlanRoute::Manual,
                result_sender,
            },
            None,
        )
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?;
    result_receiver
        .recv()
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?;
    registrar.pending_events = vec![1];
    let (tray_sender, tray_receiver) = std::sync::mpsc::sync_channel(1);
    tray_sender
        .send(WorkerCommand::ShowTrayNotice {
            kind: TrayNoticeKind::ActionRejected,
        })
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?;
    let consumer = std::thread::spawn(move || {
        let _ = tray_receiver.recv();
        tray_receiver.recv()
    });

    worker.drain_hotkey_events(
        &mut registrar,
        &crate::app::resident::AgentEventHub::default(),
        Some(&tray_sender),
        None,
    );

    assert!(matches!(
        consumer.join().ok().and_then(Result::ok),
        Some(WorkerCommand::ManualHotkeyAction { action_id }) if action_id == "screenshot-screen"
    ));
    Ok(())
}

#[test]
fn hotkey_event_notifies_tray_when_outbound_queue_is_full() -> Result<(), HotkeyError> {
    let drops = Rc::new(RefCell::new(Vec::new()));
    let mut registrar = FakeRegistrar::new(drops);
    let worker = registered_worker(&mut registrar, "capture-screen")?;
    registrar.pending_events = vec![1];
    let events = crate::app::resident::AgentEventHub::default();
    let _receiver = events.subscribe();
    for index in 0..16 {
        events.publish(&AgentOutboundEvent::TrayActionRequested {
            action_id: format!("capture-{index}"),
            invocation_id: format!("hotkey-{index}"),
            requested_at_epoch_ms: 4_000,
        });
    }
    let (tray_sender, tray_receiver) = std::sync::mpsc::sync_channel(1);

    worker.drain_hotkey_events(&mut registrar, &events, Some(&tray_sender), None);

    assert!(matches!(
        tray_receiver.try_recv(),
        Ok(WorkerCommand::ShowTrayNotice {
            kind: TrayNoticeKind::ActionRejected
        })
    ));
    Ok(())
}

fn registered_worker(
    registrar: &mut FakeRegistrar,
    action_id: &str,
) -> Result<HotkeyWorker<FakeHandle>, HotkeyError> {
    let mut worker = HotkeyWorker::default();
    let mut registry = HotkeyRegistry::default();
    let plan = registry.apply_bindings(bindings_for(&[(action_id, "Ctrl+Shift+S", true)])?)?;
    let (result_sender, result_receiver) = std::sync::mpsc::sync_channel(1);
    worker
        .handle_command(
            registrar,
            WorkerCommand::ApplyHotkeyPlan {
                plan,
                route: crate::hotkeys::HotkeyPlanRoute::Extension,
                result_sender,
            },
            None,
        )
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?;
    result_receiver
        .recv()
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?;
    Ok(worker)
}

fn bindings_for(
    actions: &[(&str, &str, bool)],
) -> Result<Vec<crate::hotkeys::HotkeyBinding>, HotkeyError> {
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
    let catalog = action_catalog_from_native_actions(&registry)
        .map_err(|_| HotkeyError::InvalidShortcutLabel)?;
    bindings_from_action_catalog(&catalog)
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
    pending_events: Vec<u32>,
}

impl FakeRegistrar {
    fn new(drops: Rc<RefCell<Vec<u32>>>) -> Self {
        Self {
            drops,
            fail_registration_id: None,
            registered: Vec::new(),
            pending_events: Vec::new(),
        }
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

impl HotkeyEventSource for FakeRegistrar {
    fn drain_hotkey_events(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.pending_events)
    }
}
