use super::AgentSideEffects;
use crate::{
    app::{
        resident::{
            capture_worker::CaptureWorkerCommandSink, hotkeys::HotkeyWorkerPlanSink,
            recording_worker::RecordingWorkerCommandSink, tray::TrayWorkerRegistrySink,
        },
        responses::{NativeHostSideEffectError, NativeHostSideEffects},
    },
    settings::{ActiveSettings, OperatingMode},
    tray::TrayNoticeKind,
};

#[test]
fn stale_extension_session_cannot_apply_any_side_effect_after_manual_epoch_is_active() {
    let mut effects = AgentSideEffects::new_with_active_settings(
        HotkeyWorkerPlanSink::new(None),
        TrayWorkerRegistrySink::new(None),
        None,
        CaptureWorkerCommandSink::new(None),
        None,
        RecordingWorkerCommandSink::new(None),
        Some(ActiveSettings::for_test_mode(OperatingMode::Manual)),
    );

    assert_eq!(
        effects.show_tray_notice(TrayNoticeKind::ActionRejected),
        Err(NativeHostSideEffectError::UnsupportedCapability)
    );
}
