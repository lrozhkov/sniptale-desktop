use crate::tray::{
    TrayActionKind, TrayMenuEntry, TrayMenuRenderEntry, TrayRecordingStatus, build_manual_menu,
    build_menu, plan_menu_render,
};

use super::{registry, registry_with_fallback_collision};

#[test]
fn disconnected_menu_keeps_only_fallback_and_offline_actions() -> Result<(), String> {
    let menu = build_menu(Some(&registry()), false, TrayRecordingStatus::Idle);
    let ids = item_ids(menu.entries());

    let TrayMenuEntry::Item(status) = &menu.entries()[0] else {
        return Err("status must be the first menu entry".to_owned());
    };
    assert_eq!(status.id(), "status");
    assert_eq!(status.label(), "Sniptale — Extension: disconnected");
    assert!(!status.enabled());
    assert_eq!(status.action_kind(), None);
    assert!(ids.contains(&"open-settings"));
    assert!(ids.contains(&"open-extension-settings"));
    assert!(!ids.contains(&"reconnect"));
    assert!(ids.contains(&"quit"));
    assert!(ids.contains(&"open-gallery"));
    assert!(ids.contains(&"screenshot-screen"));
    assert!(ids.contains(&"screenshot-active-window"));
    assert!(ids.contains(&"screenshot-all-screens"));
    assert!(ids.contains(&"screenshot-region"));
    assert!(ids.contains(&"recording-start-screen"));
    assert!(ids.contains(&"recording-start-active-window"));
    assert!(ids.contains(&"recording-start-region"));
    let extension_settings = find_item(menu.entries(), "open-extension-settings")
        .ok_or_else(|| "extension settings item must be present".to_owned())?;
    assert!(!extension_settings.enabled());
    assert!(!ids.contains(&"disabled"));
    Ok(())
}

#[test]
fn connected_menu_includes_enabled_configured_actions() -> Result<(), String> {
    let menu = build_menu(Some(&registry()), true, TrayRecordingStatus::Idle);
    let ids = item_ids(menu.entries());

    let TrayMenuEntry::Item(status) = &menu.entries()[0] else {
        return Err("status must be the first menu entry".to_owned());
    };
    assert_eq!(status.id(), "status");
    assert_eq!(status.label(), "Sniptale — Extension: connected");
    assert!(ids.contains(&"screenshot-screen"));
    assert!(ids.contains(&"screenshot-active-window"));
    assert!(ids.contains(&"recording-start-screen"));
    assert!(ids.contains(&"open-gallery"));
    assert!(ids.contains(&"open-extension-settings"));
    assert!(!ids.contains(&"disabled"));
    Ok(())
}

#[test]
fn extension_media_entries_are_disabled_while_recording() -> Result<(), String> {
    let plan = plan_menu_render(&build_menu(
        Some(&registry()),
        true,
        TrayRecordingStatus::Recording,
    ))
    .map_err(|error| error.to_string())?;

    assert!(!command_ids(&plan).contains(&"screenshot-screen"));
    assert!(!command_ids(&plan).contains(&"recording-start-screen"));
    Ok(())
}

#[test]
fn configured_menu_does_not_duplicate_fallback_action_ids() {
    let menu = build_menu(
        Some(&registry_with_fallback_collision()),
        true,
        TrayRecordingStatus::Idle,
    );
    let open_settings_count = item_ids(menu.entries())
        .into_iter()
        .filter(|id| *id == "open-settings")
        .count();

    assert_eq!(open_settings_count, 1);
    assert!(item_ids(menu.entries()).contains(&"screenshot-screen"));
}

#[test]
fn menu_render_plan_assigns_commands_only_to_enabled_actions() -> Result<(), String> {
    let menu = build_menu(Some(&registry()), true, TrayRecordingStatus::Idle);
    let plan = plan_menu_render(&menu).map_err(|error| error.to_string())?;

    let TrayMenuRenderEntry::Item(status) = &plan.entries[0] else {
        return Err("status must be rendered first".to_owned());
    };
    assert_eq!(status.id, "status");
    assert_eq!(status.command_id, None);
    assert_eq!(plan.commands[0].action_id, "screenshot-screen");
    assert_eq!(plan.commands[0].command_id, 1);
    assert!(matches!(
        plan.entries.get(3),
        Some(TrayMenuRenderEntry::Submenu {
            id: "extension-screenshot",
            ..
        })
    ));
    assert!(plan.commands.iter().any(|command| {
        command.action_id == "screenshot-screen"
            && command.action_kind == TrayActionKind::CaptureScreenshot
    }));
    Ok(())
}

#[test]
fn manual_menu_enables_only_actions_allowed_by_recording_phase() -> Result<(), String> {
    let idle = plan_menu_render(&build_manual_menu(TrayRecordingStatus::Idle))
        .map_err(|error| error.to_string())?;
    let idle_ids = command_ids(&idle);
    assert!(idle_ids.contains(&"screenshot-screen"));
    assert!(idle_ids.contains(&"recording-start-screen"));
    assert!(idle.commands.iter().any(|command| {
        command.action_id == "open-save-folder"
            && command.action_kind == TrayActionKind::OpenSaveFolder
    }));
    assert!(!idle_ids.contains(&"recording-pause"));
    assert!(!idle_ids.contains(&"recording-stop"));

    let recording = plan_menu_render(&build_manual_menu(TrayRecordingStatus::Recording))
        .map_err(|error| error.to_string())?;
    let recording_ids = command_ids(&recording);
    assert!(!recording_ids.contains(&"screenshot-screen"));
    assert!(!recording_ids.contains(&"recording-start-screen"));
    assert!(recording_ids.contains(&"recording-pause"));
    assert!(recording_ids.contains(&"recording-stop"));
    Ok(())
}

#[test]
fn mode_submenu_marks_the_active_authority_and_routes_only_the_other_choice() -> Result<(), String>
{
    let extension = build_menu(Some(&registry()), true, TrayRecordingStatus::Idle);
    let manual = build_manual_menu(TrayRecordingStatus::Idle);

    assert_mode_entries(
        extension.entries(),
        "mode-extension-managed",
        "mode-manual",
        true,
    )?;
    assert_mode_entries(
        manual.entries(),
        "mode-manual",
        "mode-extension-managed",
        true,
    )?;
    Ok(())
}

#[test]
fn mode_picker_disables_transitions_while_media_is_active() -> Result<(), String> {
    let extension = build_menu(Some(&registry()), true, TrayRecordingStatus::Recording);
    let manual = build_manual_menu(TrayRecordingStatus::Finalizing);

    assert_mode_entries(
        extension.entries(),
        "mode-extension-managed",
        "mode-manual",
        false,
    )?;
    assert_mode_entries(
        manual.entries(),
        "mode-manual",
        "mode-extension-managed",
        false,
    )?;
    Ok(())
}

fn command_ids(plan: &crate::tray::TrayMenuRenderPlan) -> Vec<&str> {
    plan.commands
        .iter()
        .map(|command| command.action_id.as_str())
        .collect()
}

fn item_ids(entries: &[TrayMenuEntry]) -> Vec<&str> {
    let mut ids = Vec::new();
    collect_item_ids(entries, &mut ids);
    ids
}

fn collect_item_ids<'a>(entries: &'a [TrayMenuEntry], ids: &mut Vec<&'a str>) {
    for entry in entries {
        match entry {
            TrayMenuEntry::Item(item) => ids.push(item.id()),
            TrayMenuEntry::Separator => {}
            TrayMenuEntry::Submenu { entries, .. } => collect_item_ids(entries, ids),
        }
    }
}

fn find_item<'a>(entries: &'a [TrayMenuEntry], id: &str) -> Option<&'a crate::tray::TrayMenuItem> {
    entries.iter().find_map(|entry| match entry {
        TrayMenuEntry::Item(item) if item.id() == id => Some(item),
        TrayMenuEntry::Item(_) | TrayMenuEntry::Separator => None,
        TrayMenuEntry::Submenu { entries, .. } => find_item(entries, id),
    })
}

fn assert_mode_entries(
    entries: &[TrayMenuEntry],
    active: &str,
    selectable: &str,
    switch_enabled: bool,
) -> Result<(), String> {
    let mode_entries = entries.iter().find_map(|entry| match entry {
        TrayMenuEntry::Submenu {
            id: "operating-mode",
            entries,
            ..
        } => Some(entries),
        _ => None,
    });
    let Some(mode_entries) = mode_entries else {
        return Err("missing mode submenu".to_owned());
    };
    let entries = mode_entries
        .iter()
        .filter_map(|entry| match entry {
            TrayMenuEntry::Item(item) => Some(item),
            TrayMenuEntry::Separator | TrayMenuEntry::Submenu { .. } => None,
        })
        .collect::<Vec<_>>();
    let active = entries
        .iter()
        .find(|item| item.id() == active)
        .ok_or_else(|| "missing active mode item".to_owned())?;
    let selectable = entries
        .iter()
        .find(|item| item.id() == selectable)
        .ok_or_else(|| "missing selectable mode item".to_owned())?;
    assert!(active.checked());
    assert!(active.radio());
    assert!(!active.enabled());
    assert!(!selectable.checked());
    assert!(selectable.radio());
    assert_eq!(selectable.enabled(), switch_enabled);
    Ok(())
}
