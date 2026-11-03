use super::*;
use crate::{
    app::responses::{
        NativeHostSharedState, effects::NoopNativeHostSideEffects,
        responses_for_message_with_effects,
    },
    protocol::{BrowserFamily, NativeControllerIdentity, parse_wire_message},
    settings::ActiveSettings,
};

fn fixture(id: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let fixtures: Value = serde_json::from_str(include_str!(
        "../../../../../contracts/native/fixtures/positive/contract-fixtures.json"
    ))?;
    Ok(fixtures[id].clone())
}

fn dispatch(
    session: &mut NativeHostSession,
    value: &Value,
) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let message = parse_wire_message(&serde_json::to_vec(value)?)?;
    Ok(responses_for_message_with_effects(
        session,
        &message,
        value,
        &mut NoopNativeHostSideEffects,
    ))
}

fn session(shared: NativeHostSharedState) -> NativeHostSession {
    let mut session = NativeHostSession::with_shared_state(shared);
    session.bind_authenticated_origin("chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/");
    session.handshake_accepted = true;
    session.extension_id = Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned());
    session
}

#[test]
fn desktop_sessions_reject_browser_settings_and_native_capture_controls()
-> Result<(), Box<dyn std::error::Error>> {
    let mut session = session(NativeHostSharedState::with_desktop_settings(
        ActiveSettings::extension_managed_default(),
    ));
    for id in [
        "ExtensionSettingsSyncMessage",
        "ExtensionScreenshotCaptureCommandMessage",
        "ExtensionRecordingStartCommandMessage",
        "ExtensionRecordingControlCommandMessage",
    ] {
        let responses = dispatch(&mut session, &fixture(id)?)?;
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0]["type"], "app.operation.failed");
        assert_eq!(responses[0]["phase"], "desktop-authority");
    }
    Ok(())
}

#[test]
fn browser_registration_never_selects_a_default_and_explicit_switch_revokes_old_lease()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let settings = ActiveSettings::extension_managed_default()
        .with_browser_target_store(directory.path().join("targets.json"))
        .map_err(|_| "registry")?;
    let shared = NativeHostSharedState::with_desktop_settings(settings.clone());
    let mut first = session(shared.clone());
    let acquire = fixture("ExtensionControllerAcquireMessage")?;
    let responses = dispatch(&mut first, &acquire)?;
    assert_eq!(responses[0]["selectionStatus"], "selection-required");
    let registry = settings.browser_targets().map_err(|_| "registry")?;
    assert_eq!(registry.default_target_id(), None);
    let first_id = registry.targets()[0].id.clone();
    settings
        .select_browser_target(Some(&first_id), false)
        .map_err(|_| "selection")?;
    let responses = dispatch(&mut first, &fixture("ExtensionPingMessage")?)?;
    let lease = responses[0]["controllerLeaseId"]
        .as_str()
        .ok_or("lease")?
        .to_owned();
    assert_eq!(responses[0]["status"], "granted");
    assert_eq!(responses[0]["selectionStatus"], "selected");
    assert_eq!(responses[1]["type"], "app.settings.accepted");
    let mut second = session(shared);
    let mut other = acquire;
    other["profileKey"] = serde_json::json!("profile-other");
    other["connectionId"] = serde_json::json!("connection-other");
    other["browserFamily"] = serde_json::json!("edge");
    assert_eq!(
        dispatch(&mut second, &other)?[0]["selectionStatus"],
        "available"
    );
    let target = crate::settings::BrowserTarget::from_controller(&NativeControllerIdentity {
        extension_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
        profile_key: "profile-other".to_owned(),
        browser_family: BrowserFamily::Edge,
        connection_id: "connection-other".to_owned(),
        document_id: None,
    })
    .map_err(|_| "target")?;
    settings
        .select_browser_target(Some(&target.id), false)
        .map_err(|_| "selection")?;
    assert_eq!(
        dispatch(&mut second, &fixture("ExtensionPingMessage")?)?[0]["status"],
        "granted"
    );
    assert!(
        !first
            .validate_active_lease(&lease, first.current_epoch_ms())
            .is_valid()
    );
    assert_eq!(
        dispatch(&mut first, &fixture("ExtensionPingMessage")?)?[0]["selectionStatus"],
        "available"
    );
    Ok(())
}

#[test]
fn replaced_connection_cannot_reacquire_the_selected_profile_on_heartbeat()
-> Result<(), Box<dyn std::error::Error>> {
    let settings = ActiveSettings::extension_managed_default();
    let shared = NativeHostSharedState::with_desktop_settings(settings.clone());
    let mut old = session(shared.clone());
    let acquire = fixture("ExtensionControllerAcquireMessage")?;
    dispatch(&mut old, &acquire)?;
    let id = settings
        .browser_targets()
        .map_err(|_| "registry")?
        .targets()[0]
        .id
        .clone();
    settings
        .select_browser_target(Some(&id), false)
        .map_err(|_| "selection")?;
    dispatch(&mut old, &acquire)?;
    let mut replacement = session(shared);
    let mut new_acquire = acquire;
    new_acquire["connectionId"] = serde_json::json!("replacement-connection");
    new_acquire["documentId"] = serde_json::json!("replacement-document");
    let response = dispatch(&mut replacement, &new_acquire)?;
    let lease = response[0]["controllerLeaseId"]
        .as_str()
        .ok_or("lease")?
        .to_owned();
    assert_eq!(response[0]["status"], "granted");
    let response = dispatch(&mut old, &fixture("ExtensionPingMessage")?)?;
    assert_eq!(response[0]["status"], "rejected");
    assert_eq!(response[0]["selectionStatus"], "available");
    assert!(
        replacement
            .validate_active_lease(&lease, replacement.current_epoch_ms())
            .is_valid()
    );
    assert_eq!(dispatch(&mut old, &new_acquire)?[0]["status"], "rejected");
    assert!(
        replacement
            .validate_active_lease(&lease, replacement.current_epoch_ms())
            .is_valid()
    );
    Ok(())
}

#[test]
fn reconnect_batches_remain_bounded_and_acked_media_does_not_accumulate() {
    struct Pending(Vec<crate::capture::PendingScreenshotTransfer>);
    impl super::super::NativeHostSideEffects for Pending {
        fn apply_hotkey_plan(
            &mut self,
            _: crate::hotkeys::HotkeyRegistrationPlan,
        ) -> Result<(), super::super::NativeHostSideEffectError> {
            Ok(())
        }
        fn pending_screenshots_for_reconnect(
            &mut self,
        ) -> Result<
            Vec<crate::capture::PendingScreenshotTransfer>,
            super::super::NativeHostSideEffectError,
        > {
            Ok(self.0.clone())
        }
    }
    let mut effects = Pending(
        (0..40)
            .map(|index| crate::capture::PendingScreenshotTransfer {
                delivery_route: None,
                capture_id: format!("capture-{index}"),
                mode: crate::protocol::ScreenshotMode::Screen,
                filename: "capture.png".to_owned(),
                total_bytes: 10,
                width: 1,
                height: 1,
                sha256: "a".repeat(64),
                chunk_count: 1,
                open_editor: false,
                captured_at_epoch_ms: 1,
            })
            .collect(),
    );
    let mut session = session(NativeHostSharedState::default());
    session.controller_lease_id = Some("lease-batch".to_owned());
    let mut responses = Vec::new();
    append_pending(&mut session, &mut effects, &mut responses);
    assert_eq!(responses.len(), 16);
    assert_eq!(session.announced_media.len(), 16);
    responses.clear();
    append_pending(&mut session, &mut effects, &mut responses);
    assert_eq!(responses, Vec::<Value>::new());
    effects.0.drain(..16);
    append_pending(&mut session, &mut effects, &mut responses);
    assert_eq!(responses.len(), 16);
    assert_eq!(session.announced_media.len(), 16);
    effects.0.clear();
    append_pending(&mut session, &mut effects, &mut responses);
    assert_eq!(
        session.announced_media,
        std::collections::BTreeSet::<String>::new()
    );
    session.announced_media.insert("old-lease-media".to_owned());
    session.release_controller_lease();
    assert_eq!(
        session.announced_media,
        std::collections::BTreeSet::<String>::new()
    );
}

#[test]
fn acknowledged_video_still_announces_its_history_in_bounded_lease_scoped_batches()
-> Result<(), Box<dyn std::error::Error>> {
    use crate::transfer::{
        trajectory_catalog::PendingTrajectory, trajectory_store::TrajectoryVideoBinding,
    };
    struct Pending(Vec<PendingTrajectory>);
    impl super::super::NativeHostSideEffects for Pending {
        fn apply_hotkey_plan(
            &mut self,
            _: crate::hotkeys::HotkeyRegistrationPlan,
        ) -> Result<(), super::super::NativeHostSideEffectError> {
            Ok(())
        }
        fn pending_recording_attachments(
            &mut self,
        ) -> Result<Vec<PendingTrajectory>, super::super::NativeHostSideEffectError> {
            Ok(self.0.clone())
        }
    }
    let descriptor: crate::protocol::trajectory::CursorTrajectoryDescriptor =
        serde_json::from_value(fixture("CursorTrajectoryDescriptor")?)?;
    let mut effects = Pending(
        (0..40)
            .map(|index| PendingTrajectory {
                recording_id: format!("recording-{index}"),
                descriptor: descriptor.clone(),
                video: Some(TrajectoryVideoBinding {
                    sha256: "a".repeat(64),
                    bytes: 100,
                    duration_ms: 1,
                    width: 10,
                    height: 10,
                }),
                video_receipt_accepted: true,
                attachment_receipt_accepted: false,
                delivery_route: crate::protocol::DeliveryRoute::DesktopDelivery {
                    asset_job_id: "job-history".to_owned(),
                    target_id: Some("a".repeat(64)),
                    ask_after_capture: false,
                },
            })
            .collect(),
    );
    let mut session = session(NativeHostSharedState::default());
    session.controller_lease_id = Some("lease-history".to_owned());
    let mut responses = Vec::new();
    append_pending(&mut session, &mut effects, &mut responses);
    assert_eq!(responses, Vec::<serde_json::Value>::new());
    session.recording_formats.cursor_trajectory = true;
    append_pending(&mut session, &mut effects, &mut responses);
    assert_eq!(responses.len(), 16);
    assert_eq!(session.announced_media.len(), 16);
    for response in &responses {
        assert_eq!(response["type"], "app.recording.attachment.available");
        assert_eq!(response["controllerLeaseId"], "lease-history");
        assert!(response.get("requestId").is_none());
        assert!(response.get("deliveryRoute").is_none());
        parse_wire_message(&serde_json::to_vec(response)?)?;
    }
    responses.clear();
    append_pending(&mut session, &mut effects, &mut responses);
    assert_eq!(responses, Vec::<serde_json::Value>::new());
    effects.0.drain(..16);
    append_pending(&mut session, &mut effects, &mut responses);
    assert_eq!(responses.len(), 16);
    assert_eq!(session.announced_media.len(), 16);
    effects.0.clear();
    append_pending(&mut session, &mut effects, &mut responses);
    assert_eq!(
        session.announced_media,
        std::collections::BTreeSet::<String>::new()
    );
    Ok(())
}

#[derive(Default)]
struct AttachmentCalls(usize);

impl super::super::NativeHostSideEffects for AttachmentCalls {
    fn apply_hotkey_plan(
        &mut self,
        _: crate::hotkeys::HotkeyRegistrationPlan,
    ) -> Result<(), super::super::NativeHostSideEffectError> {
        Ok(())
    }
    fn recording_attachment_manifest(
        &mut self,
        _: &crate::protocol::trajectory::ExtensionRecordingAttachmentManifestRequestMessage,
    ) -> Result<
        crate::protocol::trajectory::AppRecordingAttachmentManifestMessage,
        super::super::NativeHostSideEffectError,
    > {
        self.0 += 1;
        Err(super::super::NativeHostSideEffectError::RecordingTransferState)
    }
    fn recording_attachment_read_grant(
        &mut self,
        _: &crate::protocol::trajectory::ExtensionRecordingAttachmentChunkRequestMessage,
    ) -> Result<
        crate::protocol::trajectory::CursorTrajectoryReadGrant,
        super::super::NativeHostSideEffectError,
    > {
        self.0 += 1;
        Err(super::super::NativeHostSideEffectError::RecordingTransferState)
    }
    fn ack_recording_attachment(
        &mut self,
        _: &crate::protocol::trajectory::ExtensionRecordingAttachmentAckMessage,
    ) -> Result<bool, super::super::NativeHostSideEffectError> {
        self.0 += 1;
        Ok(true)
    }
    fn recording_events_manifest(
        &mut self,
        _: &crate::protocol::recording_events::ExtensionRecordingEventsManifestRequestMessage,
    ) -> Result<
        crate::protocol::recording_events::AppRecordingEventsManifestMessage,
        super::super::NativeHostSideEffectError,
    > {
        self.0 += 1;
        Err(super::super::NativeHostSideEffectError::RecordingTransferState)
    }
    fn recording_events_read_grant(
        &mut self,
        _: &crate::protocol::recording_events::ExtensionRecordingEventsChunkRequestMessage,
    ) -> Result<
        crate::protocol::recording_events::RecordingEventsReadGrant,
        super::super::NativeHostSideEffectError,
    > {
        self.0 += 1;
        Err(super::super::NativeHostSideEffectError::RecordingTransferState)
    }
    fn ack_recording_events(
        &mut self,
        _: &crate::protocol::recording_events::ExtensionRecordingEventsAckMessage,
    ) -> Result<bool, super::super::NativeHostSideEffectError> {
        self.0 += 1;
        Ok(true)
    }
}

#[test]
fn every_history_read_and_ack_checks_handshake_format_and_current_lease_before_effects()
-> Result<(), Box<dyn std::error::Error>> {
    for contract in [
        "ExtensionRecordingAttachmentManifestRequestMessage",
        "ExtensionRecordingAttachmentChunkRequestMessage",
        "ExtensionRecordingAttachmentAckMessage",
        "ExtensionRecordingEventsManifestRequestMessage",
        "ExtensionRecordingEventsChunkRequestMessage",
        "ExtensionRecordingEventsAckMessage",
    ] {
        for condition in [
            "valid",
            "no-handshake",
            "wrong-format",
            "stale-lease",
            "released",
            "poisoned",
        ] {
            let shared = NativeHostSharedState::default();
            let mut session = session(shared.clone());
            let acquired = dispatch(&mut session, &fixture("ExtensionControllerAcquireMessage")?)?;
            let lease = acquired[0]["controllerLeaseId"]
                .as_str()
                .ok_or("granted lease")?
                .to_owned();
            session.recording_formats.cursor_trajectory = true;
            session.recording_formats.recording_events = true;
            let mut value = fixture(contract)?;
            value["controllerLeaseId"] = serde_json::json!(lease);
            let expected = match condition {
                "no-handshake" => {
                    session.handshake_accepted = false;
                    "unsupported-capability"
                }
                "wrong-format" => {
                    if contract.contains("Events") {
                        session.recording_formats.recording_events = false;
                    } else {
                        session.recording_formats.cursor_trajectory = false;
                    }
                    "unsupported-capability"
                }
                "stale-lease" => {
                    value["controllerLeaseId"] = serde_json::json!("wrong-lease");
                    "stale-controller-lease"
                }
                "released" => {
                    session.release_controller_lease();
                    "stale-controller-lease"
                }
                "poisoned" => {
                    shared.poison_lease_book_for_tests();
                    "storage-failed"
                }
                _ => "",
            };
            let message = parse_wire_message(&serde_json::to_vec(&value)?)?;
            let mut effects = AttachmentCalls::default();
            let responses =
                responses_for_message_with_effects(&mut session, &message, &value, &mut effects);
            if condition == "valid" {
                assert_eq!(effects.0, 1, "{contract}");
            } else {
                assert_eq!(effects.0, 0, "{contract}: {condition}");
                assert_eq!(responses.len(), 1, "{contract}: {condition}");
                assert_eq!(
                    responses[0]["error"]["code"], expected,
                    "{contract}: {condition}"
                );
            }
        }
    }
    Ok(())
}
