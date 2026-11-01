use super::AgentSideEffects;
use crate::{
    app::{
        resident::{
            capture_worker::CaptureWorkerCommandSink, hotkeys::HotkeyWorkerPlanSink,
            recording_worker::RecordingWorkerCommandSink, tray::TrayWorkerRegistrySink,
        },
        responses::NativeHostSideEffects,
    },
    settings::{ActiveSettings, OperatingMode},
    tray::TrayNoticeKind,
};

#[test]
fn manual_delivery_session_may_project_a_category_only_notice() {
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
        Ok(())
    );
}
