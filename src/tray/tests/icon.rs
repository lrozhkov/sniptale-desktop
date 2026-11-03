use crate::tray::{
    TrayConnectionStatus, TrayIconState, TrayNoticeKind, TrayRecordingStatus, TrayStartupStatus,
    icon::state::{TrayIconCommand, TrayIconVariant, plan_icon_remove, plan_icon_update},
    manual_tray_icon_tooltip, tray_icon_tooltip, tray_icon_variant,
};

use super::icon_state;

#[test]
fn tray_icon_adds_then_updates_only_when_state_changes() {
    let state = icon_state();
    assert_eq!(
        plan_icon_update(None, &state, None),
        vec![TrayIconCommand::Add {
            tooltip: "Sniptale: connected, idle".to_owned()
        }]
    );

    assert_eq!(plan_icon_update(Some(&state), &state, None).len(), 0);

    let mut changed = state.clone();
    changed.recording = TrayRecordingStatus::Recording;
    assert_eq!(
        plan_icon_update(Some(&state), &changed, None),
        vec![TrayIconCommand::Update {
            tooltip: "Sniptale: connected, recording".to_owned()
        }]
    );
}

#[test]
fn manual_tooltip_does_not_claim_the_extension_is_disconnected() {
    let tooltip = manual_tray_icon_tooltip(TrayRecordingStatus::Idle);
    assert_eq!(tooltip, "Sniptale: Manual, idle");
    assert!(!tooltip.contains("disconnected"));
}

#[test]
fn tray_icon_variant_keeps_active_and_paused_states_distinct() {
    let mut state = icon_state();
    assert_eq!(tray_icon_variant(&state), TrayIconVariant::Normal);
    for recording in [
        TrayRecordingStatus::Preparing,
        TrayRecordingStatus::Recording,
        TrayRecordingStatus::Finalizing,
    ] {
        state.recording = recording;
        assert_eq!(tray_icon_variant(&state), TrayIconVariant::Recording);
    }
    state.recording = TrayRecordingStatus::Paused;
    assert_eq!(tray_icon_variant(&state), TrayIconVariant::Paused);
}

#[test]
fn tray_icon_notice_is_status_only_and_remove_is_idempotent() {
    let mut state = icon_state();
    state.connection = crate::tray::TrayConnectionStatus::Disconnected;
    state.startup_status = crate::tray::TrayStartupStatus::RepairRequired;

    assert_eq!(
        plan_icon_update(
            Some(&icon_state()),
            &state,
            Some(TrayNoticeKind::ActionRejected)
        ),
        vec![
            TrayIconCommand::Update {
                tooltip: "Sniptale: disconnected, idle, repair required".to_owned()
            },
            TrayIconCommand::Notice {
                kind: TrayNoticeKind::ActionRejected
            },
        ]
    );
    assert_eq!(plan_icon_remove(true), vec![TrayIconCommand::Remove]);
    assert_eq!(plan_icon_remove(false).len(), 0);
}

#[test]
fn tray_icon_tooltip_reports_typed_degraded_startup_status() {
    let mut state = icon_state();
    state.startup_status = crate::tray::TrayStartupStatus::UnsupportedCacheSchema;

    assert_eq!(
        plan_icon_update(None, &state, None),
        vec![TrayIconCommand::Add {
            tooltip: "Sniptale: connected, idle, unsupported cache schema".to_owned()
        }]
    );

    state.startup_status = crate::tray::TrayStartupStatus::RepairRequiredAndUnsupportedCacheSchema;
    assert_eq!(
        plan_icon_update(None, &state, None),
        vec![TrayIconCommand::Add {
            tooltip: "Sniptale: connected, idle, repair required, unsupported cache schema"
                .to_owned()
        }]
    );
}

#[test]
fn tray_icon_tooltip_reports_every_recording_phase() {
    for (recording, expected) in [
        (TrayRecordingStatus::Preparing, "preparing"),
        (TrayRecordingStatus::Recording, "recording"),
        (TrayRecordingStatus::Paused, "paused"),
        (TrayRecordingStatus::Finalizing, "finalizing"),
    ] {
        let state = TrayIconState {
            connection: TrayConnectionStatus::Connected,
            recording,
            startup_status: TrayStartupStatus::Operational,
        };
        assert!(tray_icon_tooltip(&state).contains(expected));
    }
}
