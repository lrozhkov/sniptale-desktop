use super::*;
use std::path::Path;
use std::sync::mpsc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::app::{
    resident::{
        WorkerCommand, capture_worker::CaptureWorkerCommandSink, hotkeys::HotkeyWorkerPlanSink,
        recording_worker::RecordingWorkerCommandSink, tray::TrayWorkerRegistrySink,
    },
    responses::{NativeHostSideEffectError, NativeHostSideEffects},
};
use crate::protocol::{DeliveryRoute, OperationAuthority};
use crate::protocol::{
    ExtensionRecordingControlCommandMessage, ExtensionRecordingStartCommandMessage,
    ExtensionScreenshotCaptureCommandMessage, NativeAudioSourceMode, NativeRecordingSource,
    NativeRequestedQualitySettings, NativeVideoFrameRate, NativeVideoQuality, PROTOCOL_VERSION,
    ScreenshotMode, VideoMode,
};
use crate::recording::{RecordingStartPolicy, RecordingTransferStore};
use crate::transfer::{
    protected_file::{PROTECTED_MEDIA_ALGORITHM, protect_file_in_place},
    protected_key::{new_protected_media_key, unprotect_media_key},
};
use crate::tray::TrayActionRegistry;

#[test]
fn agent_side_effects_project_pending_recordings_for_reconnect() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let now_ms: u64 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis()
        .try_into()
        .map_err(|error: std::num::TryFromIntError| error.to_string())?;
    write_pending_recording(temp.path(), now_ms)?;
    let store = RecordingTransferStore::open_with_default_config(temp.path())
        .map_err(|error| format!("{error:?}"))?;
    let mut effects = AgentSideEffects::new(
        HotkeyWorkerPlanSink::new(None),
        TrayWorkerRegistrySink::new(None),
        None,
        CaptureWorkerCommandSink::new(None),
        Some(store),
        RecordingWorkerCommandSink::new(None),
    );

    let pending = effects
        .pending_recordings_for_reconnect()
        .map_err(|error| format!("{error:?}"))?;

    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].recording_id, "recording-1");
    Ok(())
}

#[test]
fn agent_side_effects_enqueues_screenshot_capture() -> Result<(), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let mut effects = AgentSideEffects::new(
        HotkeyWorkerPlanSink::new(None),
        TrayWorkerRegistrySink::new(None),
        None,
        CaptureWorkerCommandSink::new(Some(sender)),
        None,
        RecordingWorkerCommandSink::new(None),
    );

    let outcome = effects
        .start_screenshot_capture(&screenshot_command())
        .map_err(|error| format!("{error:?}"))?;

    assert_eq!(
        outcome,
        crate::app::responses::NativeHostCommandOutcome::Accepted
    );
    assert!(matches!(
        receiver.try_recv(),
        Ok(WorkerCommand::CaptureScreenshot {
            request,
            extension_controller_lease_id,
        })
            if request.operation_id == "command-1"
                && request.mode == ScreenshotMode::Screen
                && request.open_editor
                && request.requested_at_epoch_ms == 4_000
                && matches!(
                    &request.authority,
                    OperationAuthority::ExtensionManaged { settings_revision }
                        if settings_revision == "settings-revision-1"
                )
                && request.delivery_route == DeliveryRoute::ExtensionTransfer
                && extension_controller_lease_id.as_deref() == Some("lease:connection-1")
    ));
    Ok(())
}

#[test]
fn agent_side_effects_maps_missing_capture_worker_to_unsupported_capability() {
    let mut effects = AgentSideEffects::new(
        HotkeyWorkerPlanSink::new(None),
        TrayWorkerRegistrySink::new(None),
        None,
        CaptureWorkerCommandSink::new(None),
        None,
        RecordingWorkerCommandSink::new(None),
    );

    assert_eq!(
        effects.start_screenshot_capture(&screenshot_command()),
        Err(NativeHostSideEffectError::UnsupportedCapability)
    );
}

#[test]
fn agent_side_effects_maps_capture_queue_full_to_backpressure() -> Result<(), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    sender
        .send(WorkerCommand::Stop)
        .map_err(|error| error.to_string())?;
    let mut effects = AgentSideEffects::new(
        HotkeyWorkerPlanSink::new(None),
        TrayWorkerRegistrySink::new(None),
        None,
        CaptureWorkerCommandSink::new(Some(sender)),
        None,
        RecordingWorkerCommandSink::new(None),
    );

    assert_eq!(
        effects.start_screenshot_capture(&screenshot_command()),
        Err(NativeHostSideEffectError::WorkerQueueFull)
    );
    drop(receiver);
    Ok(())
}

#[test]
fn agent_side_effects_maps_stopped_capture_worker_to_typed_error() {
    let (sender, receiver) = mpsc::sync_channel(1);
    drop(receiver);
    let mut effects = AgentSideEffects::new(
        HotkeyWorkerPlanSink::new(None),
        TrayWorkerRegistrySink::new(None),
        None,
        CaptureWorkerCommandSink::new(Some(sender)),
        None,
        RecordingWorkerCommandSink::new(None),
    );

    assert_eq!(
        effects.start_screenshot_capture(&screenshot_command()),
        Err(NativeHostSideEffectError::WorkerStopped)
    );
}

#[test]
fn agent_side_effects_enqueues_recording_start() -> Result<(), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let mut effects = AgentSideEffects::new(
        HotkeyWorkerPlanSink::new(None),
        TrayWorkerRegistrySink::new(None),
        None,
        CaptureWorkerCommandSink::new(None),
        None,
        RecordingWorkerCommandSink::new(Some(sender)),
    );

    let outcome = effects
        .start_recording(&recording_start_command(), RecordingStartPolicy::default())
        .map_err(|error| format!("{error:?}"))?;

    assert_eq!(
        outcome,
        crate::app::responses::NativeHostCommandOutcome::Accepted
    );
    assert!(matches!(
        receiver.try_recv(),
        Ok(WorkerCommand::StartRecording { request, policy })
            if request.operation_id == "command-1" && policy == RecordingStartPolicy::default()
    ));
    Ok(())
}

#[test]
fn agent_side_effects_enqueues_recording_control() -> Result<(), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    let mut effects = AgentSideEffects::new(
        HotkeyWorkerPlanSink::new(None),
        TrayWorkerRegistrySink::new(None),
        None,
        CaptureWorkerCommandSink::new(None),
        None,
        RecordingWorkerCommandSink::new(Some(sender)),
    );

    let outcome = effects
        .control_recording(&recording_control_command())
        .map_err(|error| format!("{error:?}"))?;

    assert_eq!(
        outcome,
        crate::app::responses::NativeHostCommandOutcome::Accepted
    );
    assert!(matches!(
        receiver.try_recv(),
        Ok(WorkerCommand::ControlRecording { request })
            if request.operation_id == "command-1"
    ));
    Ok(())
}

#[test]
fn agent_side_effects_maps_missing_recording_worker_to_unsupported_capability() {
    let mut effects = AgentSideEffects::new(
        HotkeyWorkerPlanSink::new(None),
        TrayWorkerRegistrySink::new(None),
        None,
        CaptureWorkerCommandSink::new(None),
        None,
        RecordingWorkerCommandSink::new(None),
    );

    assert_eq!(
        effects.start_recording(&recording_start_command(), RecordingStartPolicy::default()),
        Err(NativeHostSideEffectError::UnsupportedCapability)
    );
}

#[test]
fn agent_side_effects_maps_recording_queue_full_to_backpressure() -> Result<(), String> {
    let (sender, receiver) = mpsc::sync_channel(1);
    sender
        .send(WorkerCommand::Stop)
        .map_err(|error| error.to_string())?;
    let mut effects = AgentSideEffects::new(
        HotkeyWorkerPlanSink::new(None),
        TrayWorkerRegistrySink::new(None),
        None,
        CaptureWorkerCommandSink::new(None),
        None,
        RecordingWorkerCommandSink::new(Some(sender)),
    );

    assert_eq!(
        effects.start_recording(&recording_start_command(), RecordingStartPolicy::default()),
        Err(NativeHostSideEffectError::WorkerQueueFull)
    );
    drop(receiver);
    Ok(())
}

#[test]
fn agent_side_effects_maps_stopped_recording_worker_to_typed_error() {
    let (sender, receiver) = mpsc::sync_channel(1);
    drop(receiver);
    let mut effects = AgentSideEffects::new(
        HotkeyWorkerPlanSink::new(None),
        TrayWorkerRegistrySink::new(None),
        None,
        CaptureWorkerCommandSink::new(None),
        None,
        RecordingWorkerCommandSink::new(Some(sender)),
    );

    assert_eq!(
        effects.start_recording(&recording_start_command(), RecordingStartPolicy::default()),
        Err(NativeHostSideEffectError::WorkerStopped)
    );
}

#[test]
fn agent_side_effects_fail_tray_apply_when_tray_worker_is_missing() {
    let mut effects = AgentSideEffects::new(
        HotkeyWorkerPlanSink::new(None),
        TrayWorkerRegistrySink::new(None),
        None,
        CaptureWorkerCommandSink::new(None),
        None,
        RecordingWorkerCommandSink::new(None),
    );

    assert_eq!(
        effects.apply_tray_registry(TrayActionRegistry::default()),
        Err(NativeHostSideEffectError::TrayApply)
    );
}

fn write_pending_recording(root: &Path, now_ms: u64) -> Result<(), String> {
    let media_path = root.join("recording-1.mp4");
    std::fs::write(&media_path, b"hello world").map_err(|error| error.to_string())?;
    let (protection, key) =
        new_protected_media_key(PROTECTED_MEDIA_ALGORITHM).map_err(|error| format!("{error:?}"))?;
    let (_, nonce_prefix) = unprotect_media_key(&protection, PROTECTED_MEDIA_ALGORITHM)
        .map_err(|error| format!("{error:?}"))?;
    let info = protect_file_in_place(&media_path, &key, nonce_prefix)
        .map_err(|error| format!("{error:?}"))?;
    let metadata = serde_json::json!({
        "media_id": "recording-1",
        "state": "Finalized",
        "created_at_ms": now_ms.saturating_sub(2_000),
        "updated_at_ms": now_ms.saturating_sub(1_000),
        "total_bytes": info.plaintext_bytes,
        "sha256": info.plaintext_sha256,
        "retain_rejected_until_ms": null,
        "media_protection": protection,
        "recording": {
            "filename": "recording-1.mp4",
            "duration_ms": 4000,
            "width": 1920,
            "height": 1080,
            "fps": 30,
            "open_editor": true
        }
    });
    let metadata = serde_json::to_vec(&metadata).map_err(|error| error.to_string())?;
    std::fs::write(root.join("recording-1.json"), metadata).map_err(|error| error.to_string())
}

fn screenshot_command() -> ExtensionScreenshotCaptureCommandMessage {
    ExtensionScreenshotCaptureCommandMessage {
        protocol_version: PROTOCOL_VERSION,
        controller_lease_id: "lease:connection-1".to_owned(),
        command_id: "command-1".to_owned(),
        mode: ScreenshotMode::Screen,
        settings_revision: "settings-revision-1".to_owned(),
        open_editor: true,
        requested_at_epoch_ms: 4_000,
    }
}

fn recording_start_command() -> ExtensionRecordingStartCommandMessage {
    ExtensionRecordingStartCommandMessage {
        protocol_version: PROTOCOL_VERSION,
        controller_lease_id: "lease:connection-1".to_owned(),
        command_id: "command-1".to_owned(),
        source: NativeRecordingSource {
            mode: VideoMode::Screen,
            display_id: None,
            window_id: None,
            region: None,
            microphone_device_id: None,
        },
        settings_revision: "settings-revision-1".to_owned(),
        requested_quality: NativeRequestedQualitySettings {
            audio_bitrate_kbps: 160,
            audio_source_mode: NativeAudioSourceMode::Mixed,
            frame_rate: NativeVideoFrameRate::Auto,
            quality: NativeVideoQuality::High,
            video_bitrate_mbps_override: None,
        },
        open_editor: true,
        requested_at_epoch_ms: 4_000,
    }
}

fn recording_control_command() -> ExtensionRecordingControlCommandMessage {
    ExtensionRecordingControlCommandMessage {
        protocol_version: PROTOCOL_VERSION,
        controller_lease_id: "lease:connection-1".to_owned(),
        command_id: "command-1".to_owned(),
        recording_id: "recording-1".to_owned(),
        control: crate::protocol::RecordingControl::Stop,
        requested_at_epoch_ms: 4_000,
    }
}
