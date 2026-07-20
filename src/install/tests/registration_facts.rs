use super::super::{
    health::{InstallPackageIntegrity, install_runtime_health_from_facts},
    identity::InstallRuntimeIdentity,
    registration_facts::{
        BrowserFamily, CurrentRegistrationFacts, NATIVE_HOST_MANIFEST_FILE_NAME,
        NativeHostRegistryEntry, NativeHostRegistryLocation, RegistryHive, RuntimeAutostartFact,
        expected_native_host_manifest_path, native_host_registry_subkey, primary_manifest_fact,
        shadow_manifest_facts, trusted_registered_host_candidate,
    },
    registry::{InstallChannel, ManifestError, NativeHostManifest, expected_host_name},
    signature::{BinarySignatureReport, SignatureStatus},
};

const EXPECTED_ORIGIN: &str = "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/";
const EXPECTED_EXE: &str =
    r"C:\Users\Alice\AppData\Local\Sniptale\NativeAgent\sniptale-native-agent.exe";

#[test]
fn registered_host_candidate_requires_fixed_path_origin_channel_and_signature() {
    let identity =
        InstallRuntimeIdentity::for_channel_and_origins(InstallChannel::Stable, [EXPECTED_ORIGIN]);
    let valid = registration_manifest(EXPECTED_EXE);
    assert!(trusted_registered_host_candidate(
        &valid,
        EXPECTED_EXE,
        identity,
        SignatureStatus::Valid,
        false,
    ));
    let mut wrong_path = valid.clone();
    wrong_path.path = r"C:\Temp\sniptale-native-agent.exe".to_owned();
    assert!(!trusted_registered_host_candidate(
        &wrong_path,
        EXPECTED_EXE,
        identity,
        SignatureStatus::Valid,
        false,
    ));
    let mut wrong_origin = valid.clone();
    wrong_origin.allowed_origins =
        vec!["chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/".to_owned()];
    assert!(!trusted_registered_host_candidate(
        &wrong_origin,
        EXPECTED_EXE,
        identity,
        SignatureStatus::Valid,
        false,
    ));
    let mut wrong_channel = valid.clone();
    wrong_channel.channel = InstallChannel::Beta;
    assert!(!trusted_registered_host_candidate(
        &wrong_channel,
        EXPECTED_EXE,
        identity,
        SignatureStatus::Valid,
        false,
    ));
    assert!(!trusted_registered_host_candidate(
        &valid,
        EXPECTED_EXE,
        identity,
        SignatureStatus::Missing,
        false,
    ));
    assert!(trusted_registered_host_candidate(
        &valid,
        EXPECTED_EXE,
        identity,
        SignatureStatus::Missing,
        true,
    ));
}

#[test]
fn native_host_registry_subkeys_are_browser_scoped_to_host_name() {
    let host = expected_host_name(InstallChannel::Stable);

    assert_eq!(
        native_host_registry_subkey(BrowserFamily::Chrome, host),
        r"Software\Google\Chrome\NativeMessagingHosts\com.sniptale.native_host"
    );
    assert_eq!(
        native_host_registry_subkey(BrowserFamily::Edge, host),
        r"Software\Microsoft\Edge\NativeMessagingHosts\com.sniptale.native_host"
    );
    assert_eq!(
        native_host_registry_subkey(BrowserFamily::Chromium, host),
        r"Software\Chromium\NativeMessagingHosts\com.sniptale.native_host"
    );
}

#[test]
fn expected_native_host_manifest_path_lives_next_to_executable()
-> Result<(), Box<dyn std::error::Error>> {
    let path = expected_native_host_manifest_path(
        r"C:\Users\Alice\AppData\Local\Sniptale\NativeAgent\sniptale-native-agent.exe",
    )
    .ok_or("manifest path")?;

    let path = path.to_string_lossy();
    assert!(path.ends_with(NATIVE_HOST_MANIFEST_FILE_NAME));
    assert_eq!(
        path,
        r"C:\Users\Alice\AppData\Local\Sniptale\NativeAgent\native-host.json"
    );
    Ok(())
}

#[test]
fn registry_candidates_are_split_into_primary_and_shadow_manifest_facts()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let primary_path = directory.path().join("native-host.json");
    std::fs::write(&primary_path, valid_manifest_json())?;
    let drift_path = directory.path().join("old-native-host.json");
    let entries = vec![
        Ok(NativeHostRegistryEntry {
            location: NativeHostRegistryLocation {
                hive: RegistryHive::CurrentUser,
                browser: BrowserFamily::Chrome,
            },
            manifest_path: primary_path.clone(),
        }),
        Ok(NativeHostRegistryEntry {
            location: NativeHostRegistryLocation {
                hive: RegistryHive::LocalMachine,
                browser: BrowserFamily::Chrome,
            },
            manifest_path: drift_path,
        }),
    ];

    let primary = primary_manifest_fact(&entries, Some(&primary_path), InstallChannel::Stable)?;
    let shadows = shadow_manifest_facts(&entries, Some(&primary_path), InstallChannel::Stable);

    assert_eq!(
        primary.host_name,
        expected_host_name(InstallChannel::Stable)
    );
    assert_eq!(shadows, vec![Err(ManifestError::PathDrift)]);
    Ok(())
}

#[test]
fn install_runtime_health_reports_valid_signed_install_state() {
    let facts = current_registration_facts(
        registration_manifest(EXPECTED_EXE),
        SignatureStatus::Valid,
        RuntimeAutostartFact::Command(Some(format!(r#""{EXPECTED_EXE}""#))),
    );

    let health = install_runtime_health_from_facts(&facts);

    assert!(health.signed_binary);
    assert_eq!(health.package_integrity, InstallPackageIntegrity::Valid);
    assert!(health.autostart_enabled);
}

#[test]
fn install_runtime_health_keeps_unknown_signature_unknown() {
    let facts = current_registration_facts(
        registration_manifest(EXPECTED_EXE),
        SignatureStatus::Unknown,
        RuntimeAutostartFact::Unknown,
    );

    let health = install_runtime_health_from_facts(&facts);

    assert!(!health.signed_binary);
    assert_eq!(health.package_integrity, InstallPackageIntegrity::Unknown);
    assert!(!health.autostart_enabled);
}

#[test]
fn install_runtime_health_rejects_manifest_drift_even_with_valid_signature() {
    let facts = current_registration_facts(
        registration_manifest(r"C:\Temp\sniptale-native-agent.exe"),
        SignatureStatus::Valid,
        RuntimeAutostartFact::Command(Some(format!(r#""{EXPECTED_EXE}""#))),
    );

    let health = install_runtime_health_from_facts(&facts);

    assert!(health.signed_binary);
    assert_eq!(health.package_integrity, InstallPackageIntegrity::Invalid);
    assert!(health.autostart_enabled);
}

fn valid_manifest_json() -> String {
    let executable_path = EXPECTED_EXE.replace('\\', r"\\");
    format!(
        r#"{{
  "name": "{}",
  "description": "Sniptale Native Host",
  "path": "{executable_path}",
  "type": "stdio",
  "allowed_origins": ["{EXPECTED_ORIGIN}"],
  "sniptale_native_host_manifest_version": "0.1.0"
}}"#,
        expected_host_name(InstallChannel::Stable)
    )
}

fn current_registration_facts(
    manifest: NativeHostManifest,
    signature_status: SignatureStatus,
    autostart_command: RuntimeAutostartFact,
) -> CurrentRegistrationFacts {
    CurrentRegistrationFacts {
        manifest: Ok(manifest),
        shadow_manifests: vec![Err(ManifestError::MissingManifest)],
        expected_channel: InstallChannel::Stable,
        expected_executable_path: EXPECTED_EXE.to_owned(),
        expected_allowed_origins: vec![EXPECTED_ORIGIN],
        signature_report: BinarySignatureReport {
            status: signature_status,
            signer_subject: None,
        },
        autostart_command,
    }
}

fn registration_manifest(executable_path: &str) -> NativeHostManifest {
    NativeHostManifest {
        host_name: expected_host_name(InstallChannel::Stable).to_owned(),
        manifest_version: "0.1.0".to_owned(),
        allowed_origins: vec![EXPECTED_ORIGIN.to_owned()],
        path: executable_path.to_owned(),
        channel: InstallChannel::Stable,
    }
}
