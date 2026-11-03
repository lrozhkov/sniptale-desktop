use serde_json::json;

use crate::protocol::{
    NativeAudioSourceMode, NativeEffectiveEncoder, NativeRequestedQualitySettings,
    NativeSettingsSnapshot, NativeVideoFrameRate, NativeVideoQuality,
};
use proptest::{prelude::*, test_runner::TestCaseError};

use super::{
    actions::{ActionCatalogError, action_catalog_from_native_actions},
    cache::{
        CacheSchemaStatus, cache_schema_status_from_file, evaluate_cache_schema,
        native_cache_health, parse_native_cache_schema_version,
    },
    effective::{
        DEFERRED_EVALUATION_WARNING_CODE, MAX_VIDEO_BITRATE_MBPS, MIN_VIDEO_BITRATE_MBPS,
        QualityLimits, SettingsError, effective_recording_quality, effective_settings_for_sync,
    },
    snapshot::{
        AcceptedSettingsSnapshot, SettingsSnapshotError, SettingsSnapshotStore,
        max_recording_duration_ms_from_minutes,
    },
};
use crate::protocol::{NativeTrayActionDescriptor, NativeTrayActionKind, NativeTrayActionRegistry};

fn limits() -> QualityLimits {
    QualityLimits {
        max_fps: 60,
        max_width: 3840,
        max_height: 2160,
        hardware_encoder_available: true,
    }
}

fn request(quality: NativeVideoQuality) -> NativeRequestedQualitySettings {
    NativeRequestedQualitySettings {
        audio_bitrate_kbps: 128,
        audio_source_mode: NativeAudioSourceMode::Mixed,
        frame_rate: NativeVideoFrameRate::Auto,
        quality,
        video_bitrate_mbps_override: None,
    }
}

fn assert_close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 0.001);
}

#[test]
fn action_catalog_preserves_extension_actions_and_disabled_shortcuts() -> Result<(), String> {
    let catalog = action_catalog_from_native_actions(&native_action_registry(&[
        ("screenshot-screen", "Capture", Some("Ctrl+Shift+S"), true),
        (
            "screenshot-active-window",
            "Disabled",
            Some("Ctrl+Shift+D"),
            false,
        ),
    ]))
    .map_err(|error| format!("{error:?}"))?;

    assert_eq!(catalog.entries().len(), 2);
    assert_eq!(catalog.entries()[0].id, "screenshot-screen");
    assert!(catalog.entries()[0].extension_intent.is_some());
    assert!(catalog.entries()[0].enabled);
    assert_eq!(
        catalog.entries()[0].shortcut_label.as_deref(),
        Some("Ctrl+Shift+S")
    );
    assert!(!catalog.entries()[1].enabled);
    assert_eq!(
        catalog.entries()[1].shortcut_label.as_deref(),
        Some("Ctrl+Shift+D")
    );
    Ok(())
}

#[test]
fn action_catalog_rejects_duplicate_action_ids() {
    let registry = native_action_registry(&[
        ("screenshot-screen", "Capture", Some("Ctrl+Shift+S"), true),
        ("screenshot-screen", "Duplicate", Some("Ctrl+Shift+D"), true),
    ]);

    assert_eq!(
        action_catalog_from_native_actions(&registry),
        Err(ActionCatalogError::DuplicateActionId)
    );
}

#[test]
fn action_catalog_rejects_empty_revision_id_and_label() {
    let mut empty_revision =
        native_action_registry(&[("screenshot-screen", "Capture", Some("Ctrl+Shift+S"), true)]);
    empty_revision.revision.clear();
    assert_eq!(
        action_catalog_from_native_actions(&empty_revision),
        Err(ActionCatalogError::EmptyRevision)
    );

    let mut empty_id =
        native_action_registry(&[("screenshot-screen", "Capture", Some("Ctrl+Shift+S"), true)]);
    empty_id.actions[0].id.clear();
    assert_eq!(
        action_catalog_from_native_actions(&empty_id),
        Err(ActionCatalogError::EmptyActionId)
    );

    let mut empty_label =
        native_action_registry(&[("screenshot-screen", "Capture", Some("Ctrl+Shift+S"), true)]);
    empty_label.actions[0].label.clear();
    assert_eq!(
        action_catalog_from_native_actions(&empty_label),
        Err(ActionCatalogError::EmptyLabel)
    );
}

#[test]
fn action_catalog_rejects_noncanonical_or_offline_extension_media_intents() {
    let invalid = native_action_registry(&[("unknown-capture", "Capture", None, true)]);
    assert_eq!(
        action_catalog_from_native_actions(&invalid),
        Err(ActionCatalogError::InvalidExtensionIntent)
    );

    let mut offline = native_action_registry(&[("screenshot-screen", "Capture", None, true)]);
    offline.actions[0].offline_capable = Some(true);
    assert_eq!(
        action_catalog_from_native_actions(&offline),
        Err(ActionCatalogError::ExtensionIntentOfflineCapable)
    );

    let mut mismatched = native_action_registry(&[("screenshot-screen", "Capture", None, true)]);
    mismatched.actions[0].kind = NativeTrayActionKind::StartRecording;
    assert_eq!(
        action_catalog_from_native_actions(&mismatched),
        Err(ActionCatalogError::ExtensionIntentKindMismatch)
    );
}

#[test]
fn action_catalog_rejects_shell_only_ids_before_they_can_reach_the_wire() {
    let registry = NativeTrayActionRegistry {
        actions: vec![NativeTrayActionDescriptor {
            enabled: true,
            id: "recording-pause".to_owned(),
            kind: NativeTrayActionKind::OpenGallery,
            label: "Pause".to_owned(),
            shortcut_label: None,
            warning: None,
            offline_capable: Some(false),
        }],
        revision: "tray-revision-1".to_owned(),
        shortcut_priority: None,
    };

    assert_eq!(
        action_catalog_from_native_actions(&registry),
        Err(ActionCatalogError::ReservedShellActionId)
    );
}

#[test]
fn action_catalog_accepts_only_the_explicit_legacy_media_aliases() -> Result<(), String> {
    let catalog = action_catalog_from_native_actions(&native_action_registry(&[
        ("capture-screen", "Capture", None, true),
        ("capture-screenshot-active-window", "Window", None, true),
        ("capture-screenshot-region", "Region", None, true),
    ]))
    .map_err(|error| format!("{error:?}"))?;

    assert_eq!(catalog.entries()[0].id, "capture-screen");
    assert!(catalog.entries()[0].extension_intent.is_some());
    assert_eq!(catalog.entries().len(), 3);
    Ok(())
}

#[test]
fn action_catalog_ignores_the_shipped_extension_open_settings_descriptor() -> Result<(), String> {
    let mut registry = native_action_registry(&[("screenshot-screen", "Capture", None, true)]);
    registry.actions.insert(
        0,
        NativeTrayActionDescriptor {
            enabled: true,
            id: "open-settings".to_owned(),
            kind: NativeTrayActionKind::OpenSettings,
            label: "Settings".to_owned(),
            shortcut_label: None,
            warning: None,
            offline_capable: Some(true),
        },
    );

    let catalog =
        action_catalog_from_native_actions(&registry).map_err(|error| format!("{error:?}"))?;
    assert_eq!(catalog.entries().len(), 1);
    assert_eq!(catalog.entries()[0].id, "screenshot-screen");
    Ok(())
}

#[test]
fn preset_quality_preserves_geometry_and_calculates_bitrate() -> Result<(), SettingsError> {
    let effective =
        effective_recording_quality(&request(NativeVideoQuality::High), 1920, 1080, limits())?;

    assert_eq!(effective.effective_fps.as_u64(), Some(30));
    assert_close(
        effective
            .effective_video_bitrate_mbps
            .as_f64()
            .ok_or(SettingsError::InvalidNumber)?,
        8.087,
    );
    assert_eq!(effective.effective_audio_bitrate_kbps.as_u64(), Some(128));
    assert_eq!(effective.encoder, NativeEffectiveEncoder::Hardware);
    assert_eq!(effective.warnings.len(), 0);
    Ok(())
}

#[test]
fn fps_is_clamped_to_native_limit() -> Result<(), SettingsError> {
    let mut constrained = limits();
    constrained.max_fps = 24;
    let mut requested = request(NativeVideoQuality::Ultra);
    requested.frame_rate = NativeVideoFrameRate::Fps60;

    let effective = effective_recording_quality(&requested, 1920, 1080, constrained)?;

    assert_eq!(effective.effective_fps.as_u64(), Some(24));
    assert!(effective.warnings.iter().any(|warning| {
        warning.code == "clamped" && warning.field.as_deref() == Some("frameRate")
    }));
    Ok(())
}

#[test]
fn manual_bitrate_override_is_bounded() -> Result<(), SettingsError> {
    let mut low = request(NativeVideoQuality::Medium);
    low.video_bitrate_mbps_override = Some(json_number(0.5)?);
    let mut high = request(NativeVideoQuality::Medium);
    high.video_bitrate_mbps_override = Some(json_number(120.0)?);

    let low_effective = effective_recording_quality(&low, 1280, 720, limits())?;
    let high_effective = effective_recording_quality(&high, 1280, 720, limits())?;

    assert_close(
        low_effective
            .effective_video_bitrate_mbps
            .as_f64()
            .ok_or(SettingsError::InvalidNumber)?,
        2.0,
    );
    assert_close(
        high_effective
            .effective_video_bitrate_mbps
            .as_f64()
            .ok_or(SettingsError::InvalidNumber)?,
        80.0,
    );
    assert_eq!(
        low_effective.warnings[0].field.as_deref(),
        Some("videoBitrateMbpsOverride")
    );
    assert_eq!(
        high_effective.warnings[0].field.as_deref(),
        Some("videoBitrateMbpsOverride")
    );
    Ok(())
}

proptest! {
    #[test]
    fn effective_settings_policy_bounds_manual_bitrate_and_fps(
        override_mbps in 0.1_f64..120.0,
        max_fps in 1_u16..=60
    ) {
        let mut requested = request(NativeVideoQuality::Ultra);
        requested.frame_rate = NativeVideoFrameRate::Fps60;
        let bitrate_override = serde_json::Number::from_f64(override_mbps)
            .ok_or_else(|| TestCaseError::fail("generated bitrate must be finite"))?;
        requested.video_bitrate_mbps_override = Some(bitrate_override);
        let constrained = QualityLimits {
            max_fps,
            ..limits()
        };

        let effective = effective_recording_quality(&requested, 1920, 1080, constrained)
            .map_err(|error| TestCaseError::fail(format!("{error:?}")))?;

        prop_assert_eq!(effective.effective_fps.as_u64(), Some(u64::from(max_fps)));
        let bitrate = effective.effective_video_bitrate_mbps.as_f64().ok_or_else(|| {
            TestCaseError::fail("effective bitrate should remain representable as f64")
        })?;
        prop_assert!((bitrate - override_mbps.clamp(
            MIN_VIDEO_BITRATE_MBPS,
            MAX_VIDEO_BITRATE_MBPS
        )).abs() < 0.001);
    }
}

#[test]
fn software_encoder_fallback_is_reported() -> Result<(), SettingsError> {
    let mut no_hardware = limits();
    no_hardware.hardware_encoder_available = false;

    let effective =
        effective_recording_quality(&request(NativeVideoQuality::Low), 1280, 720, no_hardware)?;

    assert_eq!(effective.encoder, NativeEffectiveEncoder::Software);
    assert!(effective.warnings.iter().any(|warning| {
        warning.code == "unsupported-capability" && warning.field.as_deref() == Some("encoder")
    }));
    Ok(())
}

#[test]
fn source_dimensions_fail_instead_of_downscaling() {
    let error =
        effective_recording_quality(&request(NativeVideoQuality::Ultra), 4096, 2160, limits());

    assert!(matches!(error, Err(SettingsError::SourceExceedsCapability)));
}

#[test]
fn settings_sync_effective_settings_marks_quality_as_deferred() -> Result<(), serde_json::Error> {
    let effective = effective_settings_for_sync(&settings_snapshot(true)?);

    assert!(effective.screenshots.include_cursor);
    assert!(effective.video.enabled);
    assert_eq!(effective.warnings.len(), 1);
    assert_eq!(effective.warnings[0].code, DEFERRED_EVALUATION_WARNING_CODE);
    assert_eq!(
        effective.warnings[0].field.as_deref(),
        Some("video.effectiveQuality")
    );
    Ok(())
}

#[test]
fn settings_sync_effective_settings_does_not_emit_placeholder_when_video_disabled()
-> Result<(), serde_json::Error> {
    let effective = effective_settings_for_sync(&settings_snapshot(false)?);

    assert!(effective.screenshots.include_cursor);
    assert!(!effective.video.enabled);
    assert_eq!(effective.warnings.len(), 0);
    Ok(())
}

fn snapshot(revision: &str, schema_version: u16) -> AcceptedSettingsSnapshot {
    AcceptedSettingsSnapshot {
        controller_lease_id: "lease-1".to_owned(),
        revision: revision.to_owned(),
        schema_version,
        generated_at_epoch_ms: 1,
        max_recording_duration_ms: 240 * 60_000,
        telemetry: crate::settings::AcceptedRecordingTelemetrySettings::default(),
        settings: json!({
            "screenshots": {},
            "video": {},
            "trayActions": { "actions": [], "revision": "tray-1" }
        }),
    }
}

fn json_number(value: f64) -> Result<serde_json::Number, SettingsError> {
    serde_json::Number::from_f64(value).ok_or(SettingsError::InvalidNumber)
}

fn native_action_registry(
    actions: &[(&str, &str, Option<&str>, bool)],
) -> NativeTrayActionRegistry {
    NativeTrayActionRegistry {
        actions: actions
            .iter()
            .map(
                |(id, label, shortcut_label, enabled)| NativeTrayActionDescriptor {
                    enabled: *enabled,
                    id: (*id).to_owned(),
                    kind: NativeTrayActionKind::CaptureScreenshot,
                    label: (*label).to_owned(),
                    shortcut_label: shortcut_label.map(str::to_owned),
                    warning: None,
                    offline_capable: Some(false),
                },
            )
            .collect(),
        revision: "tray-revision-1".to_owned(),
        shortcut_priority: None,
    }
}

fn settings_snapshot(video_enabled: bool) -> Result<NativeSettingsSnapshot, serde_json::Error> {
    serde_json::from_value(json!({
        "screenshots": {
            "includeCursor": true
        },
        "video": {
            "enabled": video_enabled,
            "advanced": {
                "audioBitrateKbps": 160,
                "audioSourceMode": "mixed",
                "frameRate": "auto",
                "includeCursorInVideo": true,
                "maxDurationMinutes": 240,
                "preferHardwareEncoder": true,
                "videoBitrateMbpsOverride": null
            },
            "codec": {
                "audioCodec": "aac",
                "container": "mp4",
                "hardwareAcceleration": "prefer",
                "videoCodec": "h264"
            },
            "telemetry": {
                "collectCursor": true,
                "collectClicks": true,
                "collectKeyEvents": true,
                "collectTypingSpans": true,
                "collectStaticSignals": true
            }
        },
        "trayActions": {
            "revision": "tray-1",
            "actions": []
        }
    }))
}

#[test]
fn accepted_settings_store_keeps_last_valid_snapshot() -> Result<(), SettingsSnapshotError> {
    let mut store = SettingsSnapshotStore::default();
    store.accept(snapshot("revision-1", 1))?;

    let invalid = store.accept(snapshot("revision-2", 2));

    assert!(matches!(
        invalid,
        Err(SettingsSnapshotError::IncompatibleSchema {
            received: 2,
            supported: 1
        })
    ));
    assert_eq!(
        store.accepted().map(|accepted| accepted.revision.as_str()),
        Some("revision-1")
    );
    Ok(())
}

#[test]
fn accepted_settings_store_rejects_empty_revision_and_zero_time() {
    let mut store = SettingsSnapshotStore::default();

    assert!(matches!(
        store.accept(snapshot("", 1)),
        Err(SettingsSnapshotError::InvalidRevision)
    ));
    let mut zero_time = snapshot("revision-1", 1);
    zero_time.generated_at_epoch_ms = 0;
    assert!(matches!(
        store.accept(zero_time),
        Err(SettingsSnapshotError::InvalidGeneratedAt)
    ));
}

#[test]
fn accepted_settings_snapshot_requires_typed_recording_duration() {
    let mut store = SettingsSnapshotStore::default();
    let mut snapshot = snapshot("revision-1", 1);
    snapshot.max_recording_duration_ms = 0;

    assert!(matches!(
        store.accept(snapshot),
        Err(SettingsSnapshotError::InvalidMaxRecordingDuration)
    ));
    assert_eq!(max_recording_duration_ms_from_minutes(1.5), Ok(90_000));
    assert!(matches!(
        max_recording_duration_ms_from_minutes(721.0),
        Err(SettingsSnapshotError::InvalidMaxRecordingDuration)
    ));
}

#[test]
fn cache_schema_evaluation_fails_closed_on_newer_cache() {
    assert_eq!(evaluate_cache_schema(Some(1)), CacheSchemaStatus::Current);
    assert_eq!(
        evaluate_cache_schema(Some(0)),
        CacheSchemaStatus::NeedsReset
    );
    assert_eq!(evaluate_cache_schema(None), CacheSchemaStatus::NeedsReset);
    assert_eq!(
        evaluate_cache_schema(Some(2)),
        CacheSchemaStatus::RepairRequiredNewer {
            stored: 2,
            supported: 1
        }
    );
}

#[test]
fn native_cache_health_treats_missing_or_current_cache_as_rollback_protected() {
    assert_eq!(
        native_cache_health(CacheSchemaStatus::Current),
        super::cache::NativeCacheHealth {
            schema_status: CacheSchemaStatus::Current,
            rollback_protected: true,
            operations_supported: true,
        }
    );
    assert_eq!(
        native_cache_health(CacheSchemaStatus::NeedsReset),
        super::cache::NativeCacheHealth {
            schema_status: CacheSchemaStatus::NeedsReset,
            rollback_protected: true,
            operations_supported: true,
        }
    );
}

#[test]
fn native_cache_health_blocks_newer_or_invalid_cache() {
    for status in [
        CacheSchemaStatus::RepairRequiredNewer {
            stored: 2,
            supported: 1,
        },
        CacheSchemaStatus::RepairRequiredInvalid,
    ] {
        let health = native_cache_health(status);

        assert_eq!(health.schema_status, status);
        assert!(!health.rollback_protected);
        assert!(!health.operations_supported);
    }
}

#[test]
fn native_cache_parser_is_bounded_and_typed() {
    assert_eq!(
        parse_native_cache_schema_version(br#"{"schemaVersion":1}"#),
        Ok(1)
    );
    assert_eq!(
        parse_native_cache_schema_version(br#"{"schemaVersion":"1"}"#),
        Err(super::cache::NativeCacheReadError::InvalidJson)
    );
    let oversized = vec![b' '; 64 * 1024 + 1];
    assert_eq!(
        parse_native_cache_schema_version(&oversized),
        Err(super::cache::NativeCacheReadError::Oversize)
    );
}

#[test]
fn native_cache_file_status_maps_missing_current_newer_and_invalid()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let missing = directory.path().join("missing.json");
    assert_eq!(
        cache_schema_status_from_file(missing),
        CacheSchemaStatus::NeedsReset
    );

    let current = directory.path().join("current.json");
    std::fs::write(&current, br#"{"schemaVersion":1}"#)?;
    assert_eq!(
        cache_schema_status_from_file(current),
        CacheSchemaStatus::Current
    );

    let newer = directory.path().join("newer.json");
    std::fs::write(&newer, br#"{"schemaVersion":2}"#)?;
    assert_eq!(
        cache_schema_status_from_file(newer),
        CacheSchemaStatus::RepairRequiredNewer {
            stored: 2,
            supported: 1
        }
    );

    let invalid = directory.path().join("invalid.json");
    std::fs::write(&invalid, br#"{"schemaVersion":"new"}"#)?;
    assert_eq!(
        cache_schema_status_from_file(invalid),
        CacheSchemaStatus::RepairRequiredInvalid
    );
    Ok(())
}
