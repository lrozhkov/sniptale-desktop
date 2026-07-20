use std::{sync::mpsc, thread};

use tempfile::tempdir;

use super::activate_startup_manual_hotkeys;
use crate::{
    app::resident::{
        WorkerCommand,
        tray::{ManualHotkeyApplyError, ManualWorkerSenders},
    },
    settings::{
        ActiveSettings, LocalProfileDraft, ManualHotkeyAction, ManualProfileDraft, OperatingMode,
        apply_local_profile_draft_from_path, ensure_local_profile_from_path,
    },
};

#[test]
fn startup_rejects_manual_authority_when_hotkeys_cannot_be_registered()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let profile_path = directory.path().join("profile.json");
    let profile = ensure_local_profile_from_path(Some(profile_path.clone()))
        .map_err(|_| std::io::Error::other("profile"))?;
    let active = ActiveSettings::from_profile(&profile);
    let mut manual = ManualProfileDraft::with_defaults(
        "destination".to_owned(),
        directory.path().to_string_lossy().into_owned(),
    );
    manual
        .set_hotkey(
            ManualHotkeyAction::ScreenshotScreen,
            crate::settings::parse_manual_hotkey_chord("Ctrl+Shift+1"),
        )
        .map_err(|_| std::io::Error::other("hotkey"))?;
    apply_local_profile_draft_from_path(
        Some(profile_path),
        &active,
        &LocalProfileDraft::manual(manual),
    )
    .map_err(|_| std::io::Error::other("apply"))?;
    let (hotkey_sender, hotkey_receiver) = mpsc::sync_channel(1);
    let runtime = thread::spawn(move || {
        if let Ok(WorkerCommand::ApplyHotkeyPlan { result_sender, .. }) = hotkey_receiver.recv() {
            let _ = result_sender.send(Err(crate::hotkeys::HotkeyRuntimeError::RegisterFailed {
                registration_id: 1,
            }));
        }
    });
    let senders = ManualWorkerSenders::new(None, None, None, Some(hotkey_sender), None);

    assert!(matches!(
        activate_startup_manual_hotkeys(&active, &senders),
        Err(ManualHotkeyApplyError::Runtime(
            crate::hotkeys::HotkeyRuntimeError::RegisterFailed { registration_id: 1 }
        ))
    ));
    runtime
        .join()
        .map_err(|_| std::io::Error::other("runtime"))?;
    Ok(())
}

#[test]
fn startup_does_not_apply_retained_manual_hotkeys_in_extension_managed_mode()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let profile_path = directory.path().join("profile.json");
    let profile = ensure_local_profile_from_path(Some(profile_path.clone()))
        .map_err(|_| std::io::Error::other("profile"))?;
    let active = ActiveSettings::from_profile(&profile);
    let mut manual = ManualProfileDraft::with_defaults(
        "destination".to_owned(),
        directory.path().to_string_lossy().into_owned(),
    );
    manual
        .set_hotkey(
            ManualHotkeyAction::ScreenshotScreen,
            crate::settings::parse_manual_hotkey_chord("Ctrl+Shift+1"),
        )
        .map_err(|_| std::io::Error::other("hotkey"))?;
    apply_local_profile_draft_from_path(
        Some(profile_path),
        &active,
        &LocalProfileDraft::manual(manual),
    )
    .map_err(|_| std::io::Error::other("apply"))?;
    let retained = active
        .profile_snapshot()?
        .with_operating_mode_for_test(OperatingMode::ExtensionManaged);
    let active = ActiveSettings::from_profile(&retained);
    let (hotkey_sender, hotkey_receiver) = mpsc::sync_channel(1);
    let senders = ManualWorkerSenders::new(None, None, None, Some(hotkey_sender), None);

    assert_eq!(activate_startup_manual_hotkeys(&active, &senders), Ok(()));
    assert!(matches!(
        hotkey_receiver.try_recv(),
        Err(mpsc::TryRecvError::Empty)
    ));
    Ok(())
}
