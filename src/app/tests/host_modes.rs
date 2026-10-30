use super::*;

#[test]
fn native_host_app_hello_serializes_through_protocol_dto() -> Result<(), Box<dyn Error>> {
    let value = app_hello();
    assert_eq!(value["install"], install_state());
    let message = parse_wire_message(&serde_json::to_vec(&value)?)?;

    let WireMessage::AppHello(app_hello) = message else {
        return Err("expected app.hello message".into());
    };
    assert_eq!(app_hello.protocol_version, 1);
    assert_eq!(app_hello.install.installer_version, "0.1.0");
    assert!(app_hello.install.rollback_protected);
    assert_eq!(
        app_hello.capabilities.limits.max_chunk_bytes,
        crate::protocol::CHUNK_RAW_BYTES
    );
    Ok(())
}

#[test]
fn native_host_capability_mapper_reports_available_runtime_facts() {
    let capabilities = capabilities_from_facts(CapabilityFacts {
        capture: WgcRuntimeAvailability::Available,
        region_overlay: RegionOverlayAvailability::Available,
        recording: RecordingRuntimeAvailability::Available,
        encoder: MediaFoundationAvailability::Available,
        video_encoder: VideoEncoderAvailability {
            h264_available: true,
            h264_hardware_available: true,
        },
        audio: AudioCapabilitySummary {
            microphone_devices: vec![MicrophoneDeviceDescriptor {
                id: "mic-default".to_owned(),
                label: "Default microphone".to_owned(),
                is_default: true,
                available: true,
            }],
            supports_microphone: true,
            supports_system_audio: true,
            supports_mixed_audio: true,
            unavailable_reasons: Vec::new(),
        },
    });

    assert_eq!(
        capabilities.capture.screenshot_modes,
        vec![
            ScreenshotMode::Screen,
            ScreenshotMode::ActiveWindow,
            ScreenshotMode::AllScreens,
            ScreenshotMode::Region
        ]
    );
    assert!(capabilities.capture.supports_freeze_region_selection);
    assert_eq!(
        capabilities.capture.video_modes,
        vec![
            VideoMode::Screen,
            VideoMode::ActiveWindow,
            VideoMode::Region
        ]
    );
    assert_eq!(capabilities.codecs.containers, vec![ContainerCodec::Mp4]);
    assert_eq!(capabilities.codecs.video, vec![VideoCodec::H264]);
    assert!(capabilities.codecs.hardware_encoder_available);
    assert_eq!(capabilities.codecs.unavailable_reasons.len(), 0);
    assert!(capabilities.audio.supports_mixed_audio);
    assert_eq!(capabilities.audio.microphone_devices[0].id, "mic-default");
    assert_eq!(capabilities.audio.unavailable_reasons.len(), 0);
}

#[test]
fn native_host_capability_mapper_keeps_codecs_separate_from_recording_runtime() {
    let capabilities = capabilities_from_facts(CapabilityFacts {
        capture: WgcRuntimeAvailability::Available,
        region_overlay: RegionOverlayAvailability::RuntimeUnavailable,
        recording: RecordingRuntimeAvailability::RuntimeUnavailable,
        encoder: MediaFoundationAvailability::Available,
        video_encoder: VideoEncoderAvailability {
            h264_available: true,
            h264_hardware_available: false,
        },
        audio: AudioCapabilitySummary {
            microphone_devices: Vec::new(),
            supports_microphone: false,
            supports_system_audio: false,
            supports_mixed_audio: false,
            unavailable_reasons: Vec::new(),
        },
    });

    assert_eq!(
        capabilities.capture.screenshot_modes,
        vec![
            ScreenshotMode::Screen,
            ScreenshotMode::ActiveWindow,
            ScreenshotMode::AllScreens
        ]
    );
    assert_eq!(capabilities.capture.video_modes.len(), 0);
    assert!(!capabilities.capture.supports_freeze_region_selection);
    assert_eq!(capabilities.codecs.containers, vec![ContainerCodec::Mp4]);
    assert_eq!(capabilities.codecs.video, vec![VideoCodec::H264]);
    assert!(!capabilities.codecs.hardware_encoder_available);
    assert_eq!(capabilities.codecs.unavailable_reasons.len(), 0);
}

#[test]
fn native_host_capability_mapper_separates_region_selection_from_freeze_support() {
    let capabilities = capabilities_from_facts(CapabilityFacts {
        capture: WgcRuntimeAvailability::Available,
        region_overlay: RegionOverlayAvailability::SelectionAvailable,
        recording: RecordingRuntimeAvailability::Available,
        encoder: MediaFoundationAvailability::Available,
        video_encoder: VideoEncoderAvailability {
            h264_available: true,
            h264_hardware_available: false,
        },
        audio: AudioCapabilitySummary {
            microphone_devices: Vec::new(),
            supports_microphone: false,
            supports_system_audio: false,
            supports_mixed_audio: false,
            unavailable_reasons: Vec::new(),
        },
    });

    assert!(
        capabilities
            .capture
            .screenshot_modes
            .contains(&ScreenshotMode::Region)
    );
    assert!(
        capabilities
            .capture
            .video_modes
            .contains(&VideoMode::Region)
    );
    assert!(!capabilities.capture.supports_freeze_region_selection);
}

#[test]
fn native_host_capability_mapper_requires_capture_for_video_modes() {
    let capabilities = capabilities_from_facts(CapabilityFacts {
        capture: WgcRuntimeAvailability::ApiUnavailable,
        region_overlay: RegionOverlayAvailability::Available,
        recording: RecordingRuntimeAvailability::Available,
        encoder: MediaFoundationAvailability::Available,
        video_encoder: VideoEncoderAvailability {
            h264_available: true,
            h264_hardware_available: true,
        },
        audio: AudioCapabilitySummary {
            microphone_devices: Vec::new(),
            supports_microphone: true,
            supports_system_audio: true,
            supports_mixed_audio: true,
            unavailable_reasons: Vec::new(),
        },
    });

    assert_eq!(capabilities.capture.screenshot_modes.len(), 0);
    assert_eq!(capabilities.capture.video_modes.len(), 0);
    assert_eq!(capabilities.codecs.containers, vec![ContainerCodec::Mp4]);
    assert_eq!(capabilities.codecs.video, vec![VideoCodec::H264]);
}

#[test]
fn doctor_recording_check_requires_capture_runtime() {
    assert_eq!(
        recording_doctor_status(
            WgcRuntimeAvailability::ApiUnavailable,
            RecordingRuntimeAvailability::Available,
        ),
        DoctorStatus::Fail
    );
    assert_eq!(
        recording_doctor_detail(
            WgcRuntimeAvailability::ApiUnavailable,
            RecordingRuntimeAvailability::Available,
        ),
        "recording runtime is unavailable because capture runtime is unavailable"
    );
    assert_eq!(
        recording_doctor_status(
            WgcRuntimeAvailability::Available,
            RecordingRuntimeAvailability::Available,
        ),
        DoctorStatus::Pass
    );
}

#[test]
fn native_host_capability_mapper_requires_h264_encoder_for_video_codecs() {
    let capabilities = capabilities_from_facts(CapabilityFacts {
        capture: WgcRuntimeAvailability::Available,
        region_overlay: RegionOverlayAvailability::RuntimeUnavailable,
        recording: RecordingRuntimeAvailability::RuntimeUnavailable,
        encoder: MediaFoundationAvailability::Available,
        video_encoder: VideoEncoderAvailability {
            h264_available: false,
            h264_hardware_available: false,
        },
        audio: AudioCapabilitySummary {
            microphone_devices: Vec::new(),
            supports_microphone: false,
            supports_system_audio: false,
            supports_mixed_audio: false,
            unavailable_reasons: Vec::new(),
        },
    });

    assert_eq!(capabilities.codecs.containers.len(), 0);
    assert_eq!(capabilities.codecs.video.len(), 0);
    assert_eq!(
        capabilities.codecs.unavailable_reasons,
        vec![crate::protocol::NativeCapabilityUnavailableReason::EncoderMissing]
    );
}

#[test]
fn native_host_accepts_allowed_origin_and_parent_window_zero() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut stdin = Cursor::new(Vec::<u8>::new());

    let result = run_with_streams(
        args(&[
            "sniptale-native-agent",
            "--native-host",
            ALLOWED_NATIVE_ORIGIN,
            "--parent-window=0",
        ]),
        &mut stdin,
        &mut stdout,
        &mut stderr,
    );

    assert!(result.is_ok());
    assert_eq!(stdout.len(), 0);
    assert_eq!(stderr.len(), 0);
}

#[test]
fn native_host_accepts_browser_manifest_origin_without_mode_flag() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut stdin = Cursor::new(Vec::<u8>::new());

    let result = run_with_streams(
        args(&[
            "sniptale-native-agent",
            ALLOWED_NATIVE_ORIGIN,
            "--parent-window=0",
        ]),
        &mut stdin,
        &mut stdout,
        &mut stderr,
    );

    assert!(result.is_ok());
    assert_eq!(stdout.len(), 0);
    assert_eq!(stderr.len(), 0);
}

#[test]
fn native_host_rejects_missing_or_unknown_origin_before_stdio_bridge() {
    for launch_args in [
        vec![
            "sniptale-native-agent",
            "--native-host",
            "--parent-window=0",
        ],
        vec![
            "sniptale-native-agent",
            "--native-host",
            "chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/",
            "--parent-window=0",
        ],
        vec![
            "sniptale-native-agent",
            "chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/",
            "--parent-window=0",
        ],
    ] {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut stdin = Cursor::new(Vec::<u8>::new());

        let result = run_with_streams(
            launch_args.into_iter().map(OsString::from),
            &mut stdin,
            &mut stdout,
            &mut stderr,
        );

        assert!(result.is_err());
        assert_eq!(stdout.len(), 0);
        assert_eq!(stderr, b"unsupported argument\n");
    }
}

#[test]
fn version_json_reports_contract_versions() -> Result<(), Box<dyn Error>> {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(
        run_with_io(
            args(&["sniptale-native-agent", "--version-json"]),
            &mut stdout,
            &mut stderr
        )
        .is_ok()
    );
    let value: Value = serde_json::from_slice(&stdout)?;

    assert_eq!(stderr.len(), 0);
    assert_eq!(value["appVersion"], env!("CARGO_PKG_VERSION"));
    assert_eq!(value["protocolVersion"], 1);
    assert_eq!(value["settingsSchemaVersion"], 1);
    assert_eq!(value["appCacheSchemaVersion"], 1);
    assert_eq!(value["installerVersion"], "0.1.0");
    assert_eq!(value["nativeHostManifestVersion"], "0.1.0");
    assert_eq!(value["processMode"], "version-json");
    assert!(matches!(
        value["platformKind"].as_str(),
        Some("windows" | "macos" | "linux")
    ));
    Ok(())
}

#[test]
fn unknown_argument_fails_without_echoing_input() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let result = run_with_io(
        args(&["sniptale-native-agent", "--log-dir=/tmp/private"]),
        &mut stdout,
        &mut stderr,
    );

    assert!(result.is_err());
    assert_eq!(stdout.len(), 0);
    assert!(!String::from_utf8_lossy(&stdout).contains("logger.init.status"));
    assert_eq!(stderr, b"unsupported argument\n");
}

#[test]
fn support_mode_accepts_logging_options_without_echoing_paths() -> Result<(), Box<dyn Error>> {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(
        run_with_io(
            args(&[
                "sniptale-native-agent",
                "--doctor-json",
                "--log-level",
                "debug",
                "--log-dir",
                "C:\\Users\\Alice\\Private\\logs",
            ]),
            &mut stdout,
            &mut stderr,
        )
        .is_ok()
    );
    let value: Value = serde_json::from_slice(&stdout)?;

    assert_eq!(stderr.len(), 0);
    assert_eq!(value["processMode"], "doctor-json");
    let output = String::from_utf8(stdout)?;
    assert!(!output.contains("Alice"));
    assert!(!output.contains("Private"));
    Ok(())
}

#[test]
fn native_host_rejects_support_logging_options() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    let result = run_with_io(
        args(&[
            "sniptale-native-agent",
            "--native-host",
            "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/",
            "--log-level",
            "debug",
        ]),
        &mut stdout,
        &mut stderr,
    );

    assert!(result.is_err());
    assert_eq!(stdout.len(), 0);
    assert_eq!(stderr, b"unsupported argument\n");
}

#[test]
fn support_modes_emit_json_without_raw_paths() -> Result<(), Box<dyn Error>> {
    for (flag, process_mode) in [
        ("--repair-registration", "repair-registration"),
        ("--collect-diagnostics", "collect-diagnostics"),
    ] {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        assert!(
            run_with_io(
                args(&["sniptale-native-agent", flag]),
                &mut stdout,
                &mut stderr
            )
            .is_ok()
        );
        let value: Value = serde_json::from_slice(&stdout)?;

        assert_eq!(stderr.len(), 0);
        assert_eq!(value["processMode"], process_mode);
        assert!(!String::from_utf8_lossy(&stdout).contains(":\\Users\\"));
        if flag == "--repair-registration" {
            assert_eq!(value["status"], "repair-required");
            assert_eq!(value["repairRequired"], true);
            assert_eq!(value["replacementDecision"], "allowed");
            assert!(
                value["issues"]
                    .as_array()
                    .is_some_and(|issues| !issues.is_empty())
            );
            assert!(!String::from_utf8_lossy(&stdout).contains("sniptale-native-agent.exe"));
        }
        if flag == "--collect-diagnostics" {
            assert_eq!(value["status"], "bundle-written");
            assert_eq!(value["bundlePathCategory"], "temp-diagnostics");
            assert!(
                value["entriesWritten"]
                    .as_u64()
                    .is_some_and(|count| count >= 4)
            );
            let Some(file_name) = value["bundleFileName"].as_str() else {
                return Err(IoError::new(ErrorKind::InvalidData, "missing bundle file").into());
            };
            let bundle_path = std::env::temp_dir()
                .join("sniptale-native-agent")
                .join("diagnostics")
                .join(file_name);
            assert!(
                std::path::Path::new(file_name)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
            );
            let bundle_bytes = fs::read(&bundle_path)?;
            assert!(bundle_bytes.starts_with(b"PK\x03\x04"));
            let bundle = String::from_utf8_lossy(&bundle_bytes);
            assert!(bundle.contains("\"formatVersion\": 1"));
            assert!(bundle.contains("manifest.json"));
            assert!(bundle.contains("version.json"));
            assert!(bundle.contains("doctor.json"));
            assert!(bundle.contains("capability-snapshot.json"));
            assert!(bundle.contains("recent-errors.json"));
            assert!(bundle.contains("observability.json"));
            assert!(bundle.contains("pipe.sessions.active"));
            assert!(bundle.contains("logger.init.status"));
            assert!(!bundle.contains(":\\Users\\"));
            fs::remove_file(bundle_path)?;
        }
    }
    Ok(())
}

#[test]
fn doctor_json_reports_redacted_runtime_checks() -> Result<(), Box<dyn Error>> {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();

    assert!(
        run_with_io(
            args(&["sniptale-native-agent", "--doctor-json"]),
            &mut stdout,
            &mut stderr
        )
        .is_ok()
    );
    let value: Value = serde_json::from_slice(&stdout)?;

    assert_eq!(stderr.len(), 0);
    assert_eq!(value["processMode"], "doctor-json");
    assert_eq!(value["status"], "fail");
    assert_eq!(value["repairRequired"], true);
    assert_doctor_checks(&value)?;
    assert_doctor_observability(&value);
    assert!(!String::from_utf8_lossy(&stdout).contains(":\\Users\\"));
    Ok(())
}

fn assert_doctor_checks(value: &Value) -> Result<(), Box<dyn Error>> {
    assert!(
        value["checks"]
            .as_array()
            .is_some_and(|checks| !checks.is_empty())
    );
    let checks = value["checks"]
        .as_array()
        .ok_or_else(|| IoError::new(ErrorKind::InvalidData, "missing checks"))?;
    assert!(
        checks
            .iter()
            .any(|check| { check["name"] == "native-host-manifest" && check["status"] == "fail" })
    );
    assert!(
        checks
            .iter()
            .any(|check| check["name"] == "binary-signature" && check["status"] == "fail")
    );
    assert!(
        checks
            .iter()
            .any(|check| check["name"] == "autostart" && check["status"] == "fail")
    );
    assert!(
        checks
            .iter()
            .any(|check| check["name"] == "rollback-protection" && check["status"] == "pass")
    );
    assert!(
        checks
            .iter()
            .any(|check| check["name"] == "runtime-startup" && check["status"] == "fail")
    );
    assert!(
        checks
            .iter()
            .any(|check| check["name"] == "install-directory-acl" && check["status"] == "fail")
    );
    let runtime_access_check = checks
        .iter()
        .find(|check| check["name"] == "runtime-directory-access")
        .ok_or_else(|| IoError::new(ErrorKind::InvalidData, "missing runtime access check"))?;
    let runtime_access_categories = runtime_access_check["categories"]
        .as_array()
        .ok_or_else(|| IoError::new(ErrorKind::InvalidData, "missing runtime access categories"))?;
    assert_eq!(runtime_access_categories.len(), 4);
    assert!(
        runtime_access_categories
            .iter()
            .any(|category| category["category"] == "temp-media")
    );
    assert!(runtime_access_categories.iter().all(|category| {
        let category_name = category["category"].as_str().unwrap_or_default();
        !category_name.contains('/') && !category_name.contains('\\')
    }));
    assert!(
        checks
            .iter()
            .any(|check| check["name"] == "writable-directories" && check["status"] == "pass")
    );
    assert!(
        checks
            .iter()
            .any(|check| check["name"] == "audio-availability" && check["status"] == "fail")
    );
    assert!(
        checks
            .iter()
            .any(|check| check["name"] == "recording-availability" && check["status"] == "fail")
    );
    Ok(())
}

fn assert_doctor_observability(value: &Value) {
    assert!(
        value["observability"]["counters"]
            .as_array()
            .is_some_and(|counters| counters
                .iter()
                .any(|counter| counter["category"] == "pipe.sessions.active"))
    );
    assert!(
        value["observability"]["events"]
            .as_array()
            .is_some_and(|events| events
                .iter()
                .any(|event| event["category"] == "logger.init.status"))
    );
}
