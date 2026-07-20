use std::fs;

use tempfile::tempdir;

use super::super::manual::model::{
    ManualAudioSource, ManualFps, ManualHardwarePreference, ManualVideoQuality,
};
use super::super::{
    ActiveSettings, LocalProfileDraft, ManualProfileDraft, ModePhase, ModeTransitionError,
    OperatingMode, SettingsApplyError, apply_local_profile_draft_from_path,
    ensure_local_profile_from_path,
};

fn manual_draft(path: String) -> LocalProfileDraft {
    let mut manual =
        ManualProfileDraft::with_defaults("initial-destination".to_owned(), manual_destination());
    manual.set_destination("destination-7".to_owned(), path);
    let _ = manual.set_hotkey(
        crate::settings::ManualHotkeyAction::ScreenshotScreen,
        crate::settings::parse_manual_hotkey_chord("Ctrl+Shift+S"),
    );
    manual.set_video(
        ManualVideoQuality::Ultra,
        ManualFps::Fps60,
        Some(40),
        ManualHardwarePreference::ForceSoftware,
        false,
        7_200,
    );
    manual.set_audio(ManualAudioSource::System, None, 160);
    LocalProfileDraft::manual(manual)
}

fn manual_destination() -> String {
    std::env::temp_dir()
        .join("Sniptale-manual")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn profile_apply_persists_before_publishing_manual_epoch() -> Result<(), Box<dyn std::error::Error>>
{
    let directory = tempdir()?;
    let path = directory.path().join("local-profile.json");
    let initial = ensure_local_profile_from_path(Some(path.clone()))
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let active = ActiveSettings::from_profile(&initial);
    let draft = manual_draft(manual_destination());

    let persisted = apply_local_profile_draft_from_path(Some(path.clone()), &active, &draft)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;

    assert_eq!(persisted.operating_mode(), OperatingMode::Manual);
    assert_eq!(persisted.revision(), 2);
    assert_eq!(
        active.profile_snapshot()?.operating_mode(),
        OperatingMode::Manual
    );
    let epoch = active.epoch()?;
    assert_eq!(epoch.phase(), ModePhase::Stable(OperatingMode::Manual));
    assert_eq!(epoch.generation(), 2);
    assert!(epoch.accepts_new_operations());
    let (_, destination) = active
        .manual_destination_snapshot()
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    assert_eq!(destination.id, "destination-7");
    assert_eq!(
        destination.root,
        std::path::PathBuf::from(manual_destination())
    );
    assert_eq!(fs::read(&path)?, serde_json::to_vec(&persisted)?);
    Ok(())
}

#[test]
fn failed_profile_persistence_rolls_back_to_a_new_stable_epoch()
-> Result<(), Box<dyn std::error::Error>> {
    let initial = ensure_local_profile_from_path(None)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let active = ActiveSettings::from_profile(&initial);

    assert_eq!(
        apply_local_profile_draft_from_path(None, &active, &manual_draft(manual_destination())),
        Err(SettingsApplyError::InvalidProfile(
            super::LocalProfileError::Io
        ))
    );

    let epoch = active.epoch()?;
    assert_eq!(
        epoch.phase(),
        ModePhase::Stable(OperatingMode::ExtensionManaged)
    );
    assert_eq!(epoch.generation(), 2);
    assert!(epoch.accepts_new_operations());
    Ok(())
}

#[test]
fn invalid_manual_draft_does_not_fence_the_current_authority()
-> Result<(), Box<dyn std::error::Error>> {
    let initial = ensure_local_profile_from_path(None)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let active = ActiveSettings::from_profile(&initial);
    let invalid = manual_draft("relative-output".to_owned());

    assert!(matches!(
        active.prepare_profile_transition(&invalid),
        Err(SettingsApplyError::InvalidProfile(_))
    ));
    assert_eq!(
        active.epoch()?.phase(),
        ModePhase::Stable(OperatingMode::ExtensionManaged)
    );
    assert_eq!(active.epoch()?.generation(), 1);
    Ok(())
}

#[test]
fn switching_epoch_rejects_reentry_and_degrades_fail_closed_after_commit_boundary()
-> Result<(), Box<dyn std::error::Error>> {
    let initial = ensure_local_profile_from_path(None)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let active = ActiveSettings::from_profile(&initial);
    let transition = active.prepare_profile_transition(&manual_draft(manual_destination()))?;

    assert!(matches!(
        active.prepare_profile_transition(&LocalProfileDraft::extension_managed()),
        Err(SettingsApplyError::Mode(_))
    ));
    assert!(matches!(
        active.epoch()?.phase(),
        ModePhase::Switching {
            previous: OperatingMode::ExtensionManaged,
            target: OperatingMode::Manual,
        }
    ));
    assert!(!active.epoch()?.accepts_new_operations());

    let degraded = active.degrade_after_persistence(&transition)?;
    assert_eq!(degraded.phase(), ModePhase::Degraded(OperatingMode::Manual));
    assert!(!degraded.accepts_new_operations());
    Ok(())
}

#[test]
fn profile_draft_from_existing_profile_preserves_manual_settings_for_editing()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("local-profile.json");
    let initial = ensure_local_profile_from_path(Some(path.clone()))
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let active = ActiveSettings::from_profile(&initial);
    apply_local_profile_draft_from_path(Some(path), &active, &manual_draft(manual_destination()))
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;

    let editing = LocalProfileDraft::from(&active.profile_snapshot()?);
    let transition = active.prepare_profile_transition(&editing)?;
    active.rollback_profile_transition(&transition)?;
    assert_eq!(active.profile_snapshot()?.revision(), 2);
    Ok(())
}

#[test]
fn extension_profile_reports_a_typed_manual_destination_failure()
-> Result<(), Box<dyn std::error::Error>> {
    let profile = ensure_local_profile_from_path(None)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let active = ActiveSettings::from_profile(&profile);
    let Err(error) = active.manual_destination_snapshot() else {
        return Err(std::io::Error::other("manual destination unexpectedly available").into());
    };
    assert_eq!(error, SettingsApplyError::RuntimeUnavailable);
    assert_eq!(error.to_string(), "runtime-unavailable");
    assert!(
        active
            .profile_snapshot()?
            .manual_recording_settings()
            .is_none()
    );
    Ok(())
}

#[test]
fn committed_epoch_degrades_fail_closed_when_runtime_publish_is_incomplete()
-> Result<(), Box<dyn std::error::Error>> {
    let profile = ensure_local_profile_from_path(None)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
    let active = ActiveSettings::from_profile(&profile);
    let transition = active.prepare_profile_transition(&manual_draft(manual_destination()))?;
    let stable = active.commit_profile_transition(&transition)?;

    let degraded = active.degrade_stable(stable)?;
    assert_eq!(degraded.phase(), ModePhase::Degraded(OperatingMode::Manual));
    assert!(!active.allows_extension_ingress());
    assert_eq!(
        SettingsApplyError::Mode(ModeTransitionError::StaleEpoch).to_string(),
        "mode-transition"
    );
    Ok(())
}
