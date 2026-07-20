use std::fs;

use serde_json::json;
use tempfile::tempdir;

use super::super::profile_store::ensure_local_profile_from_path;
use super::{
    LocalProfileError, LocalProfileHealth, OperatingMode, extension_admission_allowed_from_path,
    local_profile_file_name, local_profile_health_from_path, parse_local_profile,
};

fn profile(mode: &str) -> serde_json::Value {
    let destination = std::env::temp_dir()
        .join("Sniptale")
        .to_string_lossy()
        .into_owned();
    json!({
        "schemaVersion": 1,
        "revision": 1,
        "operatingMode": mode,
        "manualProfile": {
            "destination": { "id": "destination-1", "path": destination },
            "screenshot": { "includeCursor": true },
            "video": {
                "quality": "high", "fps": "fps30", "bitrateOverrideMbps": null,
                "hardwarePreference": "prefer", "includeCursor": true,
                "maxDurationSeconds": 3600
            },
            "audio": {
                "sourceMode": "mixed", "microphoneDeviceId": "microphone-1",
                "bitrateKbps": 128
            },
            "hotkeys": { "screenshot-screen": "Ctrl+Shift+S" }
        }
    })
}

#[test]
fn missing_profile_defaults_to_extension_managed_health() {
    assert_eq!(
        local_profile_health_from_path(None),
        LocalProfileHealth::Missing
    );
}

#[test]
fn missing_profile_is_atomically_persisted_as_extension_managed()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("local-profile.json");

    let profile = ensure_local_profile_from_path(Some(path.clone()))
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;

    assert_eq!(profile.operating_mode(), OperatingMode::ExtensionManaged);
    assert_eq!(
        local_profile_health_from_path(Some(path.clone())),
        LocalProfileHealth::Available(OperatingMode::ExtensionManaged)
    );
    assert!(!path.with_extension("json.pending").exists());
    Ok(())
}

#[test]
fn missing_runtime_path_uses_in_memory_extension_default() -> Result<(), Box<dyn std::error::Error>>
{
    let profile = ensure_local_profile_from_path(None)
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;

    assert_eq!(profile.operating_mode(), OperatingMode::ExtensionManaged);
    Ok(())
}

#[test]
fn corrupt_profile_is_not_replaced_by_default_recovery() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("local-profile.json");
    fs::write(&path, b"not-json")?;

    assert_eq!(
        ensure_local_profile_from_path(Some(path.clone())),
        Err(LocalProfileError::InvalidJson)
    );
    assert_eq!(fs::read(&path)?, b"not-json");
    Ok(())
}

#[test]
fn existing_valid_profile_is_loaded_without_rewrite() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("local-profile.json");
    let payload = serde_json::to_vec(&profile("manual"))?;
    fs::write(&path, &payload)?;

    let loaded = ensure_local_profile_from_path(Some(path.clone()))
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;

    assert_eq!(loaded.operating_mode(), OperatingMode::Manual);
    assert_eq!(fs::read(&path)?, payload);
    Ok(())
}

#[test]
fn stale_private_profile_pending_file_does_not_block_default_recovery()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("local-profile.json");
    let pending = path.with_extension("json.pending");
    fs::write(&pending, b"partial")?;

    let profile = ensure_local_profile_from_path(Some(path.clone()))
        .map_err(|error| std::io::Error::other(format!("{error:?}")))?;

    assert_eq!(profile.operating_mode(), OperatingMode::ExtensionManaged);
    assert!(path.is_file());
    assert!(!pending.exists());
    Ok(())
}

#[test]
fn missing_path_and_read_failure_have_typed_health_without_path_echo()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    assert_eq!(
        local_profile_health_from_path(Some(directory.path().join("missing.json"))),
        LocalProfileHealth::Missing
    );
    assert_eq!(
        local_profile_health_from_path(Some(directory.path().to_path_buf())),
        LocalProfileHealth::RepairRequired(LocalProfileError::Io)
    );
    assert_eq!(local_profile_file_name(), "local-profile.json");
    Ok(())
}

#[test]
fn valid_manual_profile_is_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("local-profile.json");
    fs::write(&path, serde_json::to_vec(&profile("manual"))?)?;

    assert_eq!(
        local_profile_health_from_path(Some(path)),
        LocalProfileHealth::Available(OperatingMode::Manual)
    );
    Ok(())
}

#[test]
fn profile_rejects_malformed_and_duplicate_manual_hotkeys() -> Result<(), Box<dyn std::error::Error>>
{
    let mut malformed = profile("manual");
    malformed["manualProfile"]["hotkeys"] = json!({ "screenshot-screen": "Ctrl++S" });
    assert_eq!(
        parse_local_profile(&serde_json::to_vec(&malformed)?),
        Err(LocalProfileError::InvalidProfile)
    );

    let mut duplicate = profile("manual");
    duplicate["manualProfile"]["hotkeys"] = json!({
        "screenshot-screen": "Ctrl+Shift+S",
        "recording-stop": "control+shift+s"
    });
    assert_eq!(
        parse_local_profile(&serde_json::to_vec(&duplicate)?),
        Err(LocalProfileError::InvalidProfile)
    );
    Ok(())
}

#[test]
fn extension_admission_is_closed_for_manual_or_repair_required_profiles()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let manual_path = directory.path().join("manual-profile.json");
    fs::write(&manual_path, serde_json::to_vec(&profile("manual"))?)?;
    assert!(!extension_admission_allowed_from_path(Some(manual_path)));

    let invalid_path = directory.path().join("invalid-profile.json");
    fs::write(&invalid_path, b"not-json")?;
    assert!(!extension_admission_allowed_from_path(Some(invalid_path)));
    assert!(extension_admission_allowed_from_path(None));
    Ok(())
}

#[test]
fn profile_rejects_unknown_shape_newer_schema_and_incomplete_manual_mode()
-> Result<(), Box<dyn std::error::Error>> {
    let mut unknown = profile("extension-managed");
    unknown["unexpected"] = json!(true);
    assert_eq!(
        parse_local_profile(&serde_json::to_vec(&unknown)?),
        Err(LocalProfileError::InvalidJson)
    );

    let mut newer = profile("extension-managed");
    newer["schemaVersion"] = json!(2);
    assert_eq!(
        parse_local_profile(&serde_json::to_vec(&newer)?),
        Err(LocalProfileError::UnsupportedSchema)
    );

    let mut incomplete = profile("manual");
    incomplete["manualProfile"] = serde_json::Value::Null;
    assert_eq!(
        parse_local_profile(&serde_json::to_vec(&incomplete)?),
        Err(LocalProfileError::InvalidProfile)
    );

    let mut zero_revision = profile("extension-managed");
    zero_revision["revision"] = json!(0);
    assert_eq!(
        parse_local_profile(&serde_json::to_vec(&zero_revision)?),
        Err(LocalProfileError::InvalidProfile)
    );
    Ok(())
}

#[test]
fn extension_managed_profile_can_retain_no_manual_draft() -> Result<(), Box<dyn std::error::Error>>
{
    let mut extension_managed = profile("extension-managed");
    extension_managed["manualProfile"] = serde_json::Value::Null;
    let directory = tempdir()?;
    let path = directory.path().join("extension-profile.json");
    fs::write(&path, serde_json::to_vec(&extension_managed)?)?;

    let health = local_profile_health_from_path(Some(path));
    assert_eq!(
        health,
        LocalProfileHealth::Available(OperatingMode::ExtensionManaged)
    );
    assert!(matches!(health, LocalProfileHealth::Available(_)));
    assert!(matches!(
        LocalProfileHealth::RepairRequired(LocalProfileError::InvalidJson),
        LocalProfileHealth::RepairRequired(_)
    ));
    Ok(())
}

#[test]
fn profile_rejects_unsafe_destination_and_out_of_range_settings()
-> Result<(), Box<dyn std::error::Error>> {
    let mut unsafe_path = profile("manual");
    unsafe_path["manualProfile"]["destination"]["path"] = json!("relative/output");
    assert_eq!(
        parse_local_profile(&serde_json::to_vec(&unsafe_path)?),
        Err(LocalProfileError::InvalidProfile)
    );

    let mut network_path = profile("manual");
    network_path["manualProfile"]["destination"]["path"] = json!("//server/share");
    assert_eq!(
        parse_local_profile(&serde_json::to_vec(&network_path)?),
        Err(LocalProfileError::InvalidProfile)
    );

    let mut traversal_path = profile("manual");
    let traversal = std::env::temp_dir()
        .join("Sniptale")
        .join("..")
        .join("other")
        .to_string_lossy()
        .into_owned();
    traversal_path["manualProfile"]["destination"]["path"] = json!(traversal);
    assert_eq!(
        parse_local_profile(&serde_json::to_vec(&traversal_path)?),
        Err(LocalProfileError::InvalidProfile)
    );

    let mut invalid_audio = profile("manual");
    invalid_audio["manualProfile"]["audio"]["bitrateKbps"] = json!(95);
    assert_eq!(
        parse_local_profile(&serde_json::to_vec(&invalid_audio)?),
        Err(LocalProfileError::InvalidProfile)
    );
    Ok(())
}

#[test]
fn profile_is_bounded_before_json_parse() {
    let oversized = vec![b'x'; 64 * 1024 + 1];
    assert_eq!(
        parse_local_profile(&oversized),
        Err(LocalProfileError::Oversize)
    );
}

#[test]
fn profile_file_is_bounded_before_reading_its_full_payload()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempdir()?;
    let path = directory.path().join("oversized-profile.json");
    fs::write(&path, vec![b'x'; 64 * 1024 + 1])?;

    assert_eq!(
        local_profile_health_from_path(Some(path)),
        LocalProfileHealth::RepairRequired(LocalProfileError::Oversize)
    );
    Ok(())
}

#[test]
fn profile_accepts_unicode_and_long_absolute_destination() -> Result<(), Box<dyn std::error::Error>>
{
    let mut unicode = profile("manual");
    let long_component = "длинный-путь-".repeat(40);
    let destination = std::env::temp_dir()
        .join("Скриншоты")
        .join(long_component)
        .to_string_lossy()
        .into_owned();
    unicode["manualProfile"]["destination"]["path"] = json!(destination);

    assert!(parse_local_profile(&serde_json::to_vec(&unicode)?).is_ok());
    Ok(())
}
