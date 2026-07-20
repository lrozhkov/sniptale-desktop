use super::super::identity::InstallRuntimeIdentity;
use super::super::manifest_writer::{
    ManifestTemplateError, NativeHostManifestTemplate, render_native_host_manifest,
};
use super::super::registration_facts::{CurrentRegistrationFacts, RuntimeAutostartFact};
use super::super::registry::{
    InstallChannel, ManifestError, NativeHostManifest, expected_host_name, expected_install_id,
    parse_manifest_json, read_manifest_file, validate_manifest,
};
use super::super::runtime_identity_gate::verify_runtime_manifest_identity;
use super::super::signature::{SignatureProbeResult, signature_report_from_probe};

const EXPECTED_ORIGIN: &str = "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/";
const EXPECTED_PATH: &str =
    "C:\\Users\\Alice\\AppData\\Local\\Sniptale\\NativeAgent\\sniptale-native-agent.exe";

fn manifest() -> NativeHostManifest {
    NativeHostManifest {
        host_name: expected_host_name(InstallChannel::Stable).to_owned(),
        manifest_version: "0.1.0".to_owned(),
        allowed_origins: vec![EXPECTED_ORIGIN.to_owned()],
        path: EXPECTED_PATH.to_owned(),
        channel: InstallChannel::Stable,
    }
}

fn validate(manifest: &NativeHostManifest) -> Result<(), ManifestError> {
    let identity =
        InstallRuntimeIdentity::for_channel_and_origins(InstallChannel::Stable, [EXPECTED_ORIGIN]);
    validate_manifest(
        manifest,
        identity.channel,
        EXPECTED_PATH,
        &identity.allowed_origins,
    )
}

fn registration_facts(manifest: NativeHostManifest) -> CurrentRegistrationFacts {
    CurrentRegistrationFacts {
        manifest: Ok(manifest),
        shadow_manifests: Vec::new(),
        expected_channel: InstallChannel::Stable,
        expected_executable_path: EXPECTED_PATH.to_owned(),
        expected_allowed_origins: vec![EXPECTED_ORIGIN],
        signature_report: signature_report_from_probe(SignatureProbeResult::Valid),
        autostart_command: RuntimeAutostartFact::Unknown,
    }
}

fn registration_facts_with_shadow(
    manifest: NativeHostManifest,
    shadow_manifest: NativeHostManifest,
) -> CurrentRegistrationFacts {
    let mut facts = registration_facts(manifest);
    facts.shadow_manifests.push(Ok(shadow_manifest));
    facts
}

#[test]
fn install_runtime_identity_is_channel_scoped() {
    for (channel, install_id, host_name) in [
        (
            InstallChannel::Stable,
            "install-stable",
            "com.sniptale.native_host",
        ),
        (
            InstallChannel::Beta,
            "install-beta",
            "com.sniptale.beta.native_host",
        ),
        (
            InstallChannel::Dev,
            "install-dev",
            "com.sniptale.dev.native_host",
        ),
    ] {
        let identity = InstallRuntimeIdentity::for_channel_and_origins(channel, [EXPECTED_ORIGIN]);

        assert_eq!(identity.channel, channel);
        assert_eq!(identity.install_id, install_id);
        assert_eq!(identity.install_id, expected_install_id(channel));
        assert_eq!(identity.host_name, host_name);
        assert_eq!(identity.host_name, expected_host_name(channel));
        assert_eq!(identity.allowed_origins, [EXPECTED_ORIGIN]);
        assert!(identity.allows_extension_origin(EXPECTED_ORIGIN));
        assert!(
            !identity
                .allows_extension_origin("chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/")
        );
    }
}

#[test]
fn valid_manifest_passes_trust_checks() -> Result<(), ManifestError> {
    validate(&manifest())
}

#[test]
fn valid_manifest_json_parses_to_typed_manifest() -> Result<(), ManifestError> {
    let manifest = parse_manifest_json(valid_manifest_json().as_bytes(), InstallChannel::Stable)?;

    assert_eq!(
        manifest.host_name,
        expected_host_name(InstallChannel::Stable)
    );
    assert_eq!(manifest.manifest_version, "0.1.0");
    assert_eq!(manifest.path, EXPECTED_PATH);
    assert_eq!(manifest.allowed_origins, vec![EXPECTED_ORIGIN.to_owned()]);
    validate(&manifest)
}

#[test]
fn rendered_manifest_round_trips_to_valid_typed_manifest() -> Result<(), Box<dyn std::error::Error>>
{
    let payload = render_native_host_manifest(NativeHostManifestTemplate {
        channel: InstallChannel::Stable,
        executable_path: EXPECTED_PATH,
        allowed_origins: &[EXPECTED_ORIGIN],
    })?;

    let manifest = parse_manifest_json(payload.as_bytes(), InstallChannel::Stable)?;

    assert_eq!(
        manifest.host_name,
        expected_host_name(InstallChannel::Stable)
    );
    assert_eq!(manifest.manifest_version, "0.1.0");
    assert_eq!(manifest.path, EXPECTED_PATH);
    assert_eq!(manifest.allowed_origins, vec![EXPECTED_ORIGIN.to_owned()]);
    validate(&manifest)?;
    Ok(())
}

#[test]
fn rendered_manifest_matches_runtime_identity_for_each_channel()
-> Result<(), Box<dyn std::error::Error>> {
    for channel in [
        InstallChannel::Stable,
        InstallChannel::Beta,
        InstallChannel::Dev,
    ] {
        let identity = InstallRuntimeIdentity::for_channel_and_origins(channel, [EXPECTED_ORIGIN]);
        let payload = render_native_host_manifest(NativeHostManifestTemplate {
            channel: identity.channel,
            executable_path: EXPECTED_PATH,
            allowed_origins: &identity.allowed_origins,
        })?;
        let manifest = parse_manifest_json(payload.as_bytes(), identity.channel)?;

        assert_eq!(manifest.channel, identity.channel);
        assert_eq!(manifest.host_name, identity.host_name);
        assert_eq!(
            manifest.allowed_origins,
            identity
                .allowed_origins
                .iter()
                .map(|origin| (*origin).to_owned())
                .collect::<Vec<_>>()
        );
        validate_manifest(
            &manifest,
            identity.channel,
            EXPECTED_PATH,
            &identity.allowed_origins,
        )?;
    }
    Ok(())
}

#[test]
fn manifest_rendering_fails_closed_for_empty_path_and_invalid_origins() {
    assert_eq!(
        render_native_host_manifest(NativeHostManifestTemplate {
            channel: InstallChannel::Stable,
            executable_path: " ",
            allowed_origins: &[EXPECTED_ORIGIN],
        }),
        Err(ManifestTemplateError::EmptyExecutablePath)
    );

    assert_eq!(
        render_native_host_manifest(NativeHostManifestTemplate {
            channel: InstallChannel::Stable,
            executable_path: EXPECTED_PATH,
            allowed_origins: &["*"],
        }),
        Err(ManifestTemplateError::InvalidManifest(
            ManifestError::InvalidAllowedOrigin
        ))
    );
}

#[test]
fn manifest_file_reader_maps_missing_and_valid_files() -> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let missing = directory.path().join("missing.json");
    assert_eq!(
        read_manifest_file(&missing, InstallChannel::Stable),
        Err(ManifestError::MissingManifest)
    );

    let manifest_path = directory.path().join("native-host.json");
    std::fs::write(&manifest_path, valid_manifest_json())?;
    let manifest = read_manifest_file(&manifest_path, InstallChannel::Stable)?;

    assert_eq!(
        manifest.host_name,
        expected_host_name(InstallChannel::Stable)
    );
    Ok(())
}

#[test]
fn manifest_json_rejects_malformed_or_oversize_payloads() {
    assert_eq!(
        parse_manifest_json(
            br#"{"name":"com.sniptale.native_host""#,
            InstallChannel::Stable
        ),
        Err(ManifestError::InvalidJson)
    );

    let oversized = vec![b' '; super::super::registry::MAX_NATIVE_HOST_MANIFEST_BYTES + 1];
    assert_eq!(
        parse_manifest_json(&oversized, InstallChannel::Stable),
        Err(ManifestError::OversizeManifest)
    );
}

#[test]
fn manifest_json_rejects_wrong_type_version_channel_and_origins() {
    for (payload, error) in [
        (
            valid_manifest_json().replace(r#""type":"stdio""#, r#""type":"stdio-dev""#),
            ManifestError::UnsupportedManifestType,
        ),
        (
            valid_manifest_json().replace(
                r#""sniptale_native_host_manifest_version":"0.1.0""#,
                r#""sniptale_native_host_manifest_version":"0.0.1""#,
            ),
            ManifestError::ManifestVersionDrift,
        ),
        (
            valid_manifest_json()
                .replace("com.sniptale.native_host", "com.sniptale.beta.native_host"),
            ManifestError::InvalidHostName,
        ),
        (
            valid_manifest_json().replace(
                r#""chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/""#,
                r#""*""#,
            ),
            ManifestError::InvalidAllowedOrigin,
        ),
    ] {
        assert_eq!(
            parse_manifest_json(payload.as_bytes(), InstallChannel::Stable),
            Err(error)
        );
    }
}

#[test]
fn host_name_rules_reject_ambiguous_names() {
    for host_name in [
        ".com.sniptale",
        "com..sniptale",
        "com.sniptale.",
        "Com.Sniptale",
        "com-sniptale",
        "com.sniptale.native_agent",
        "com.sniptale.beta.native_host",
    ] {
        let mut manifest = manifest();
        manifest.host_name = host_name.to_owned();
        assert!(matches!(
            validate(&manifest),
            Err(ManifestError::InvalidHostName)
        ));
    }
}

#[test]
fn configured_origins_and_channel_must_match() {
    let mut stable = manifest();
    stable.allowed_origins =
        vec!["chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/".to_owned()];
    assert!(matches!(
        validate(&stable),
        Err(ManifestError::UnexpectedAllowedOrigin)
    ));

    let mut beta = manifest();
    beta.channel = InstallChannel::Beta;
    beta.host_name = expected_host_name(InstallChannel::Beta).to_owned();
    assert!(matches!(validate(&beta), Err(ManifestError::WrongChannel)));
}

#[test]
fn runtime_manifest_identity_gate_rejects_wrong_channel_before_pipe_auth() {
    let mut manifest = manifest();
    manifest.channel = InstallChannel::Beta;
    manifest.host_name = expected_host_name(InstallChannel::Beta).to_owned();
    let facts = registration_facts(manifest);

    assert_eq!(
        verify_runtime_manifest_identity(&facts),
        Err(ManifestError::WrongChannel)
    );
}

#[test]
fn runtime_manifest_identity_gate_rejects_wrong_origin_before_pipe_auth() {
    let mut manifest = manifest();
    manifest.allowed_origins =
        vec!["chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/".to_owned()];
    let facts = registration_facts(manifest);

    assert_eq!(
        verify_runtime_manifest_identity(&facts),
        Err(ManifestError::UnexpectedAllowedOrigin)
    );
}

#[test]
fn runtime_manifest_identity_gate_rejects_shadow_manifest_drift_before_pipe_auth() {
    let primary = manifest();
    let mut shadow = manifest();
    shadow.allowed_origins =
        vec!["chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb/".to_owned()];
    let facts = registration_facts_with_shadow(primary, shadow);

    assert_eq!(
        verify_runtime_manifest_identity(&facts),
        Err(ManifestError::UnexpectedAllowedOrigin)
    );
}

#[test]
fn runtime_manifest_identity_gate_rejects_missing_manifest_before_pipe_auth() {
    let mut facts = registration_facts(manifest());
    facts.manifest = Err(ManifestError::MissingManifest);

    assert_eq!(
        verify_runtime_manifest_identity(&facts),
        Err(ManifestError::MissingManifest)
    );
}

#[test]
fn runtime_manifest_identity_gate_rejects_path_drift_before_pipe_auth() {
    let mut manifest = manifest();
    manifest.path = "C:\\Temp\\sniptale-native-agent.exe".to_owned();
    let facts = registration_facts(manifest);

    assert_eq!(
        verify_runtime_manifest_identity(&facts),
        Err(ManifestError::PathDrift)
    );
}

#[test]
fn path_drift_is_repair_required_but_packaged_relative_exe_name_is_allowed() {
    let base_manifest = manifest();

    assert!(matches!(
        validate_manifest(
            &base_manifest,
            InstallChannel::Stable,
            "C:\\Temp\\sniptale-native-agent.exe",
            &[EXPECTED_ORIGIN]
        ),
        Err(ManifestError::PathDrift)
    ));

    let mut packaged = manifest();
    packaged.path = "sniptale-native-agent.exe".to_owned();
    assert_eq!(validate(&packaged), Ok(()));
}

#[test]
fn origins_must_be_explicit_extension_urls() {
    let mut manifest = manifest();
    manifest.allowed_origins = vec![
        "*".to_owned(),
        "https://example.invalid".to_owned(),
        "chrome-extension://short/".to_owned(),
        "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaq/".to_owned(),
    ];

    assert!(matches!(
        validate(&manifest),
        Err(ManifestError::InvalidAllowedOrigin)
    ));
}

#[test]
fn origin_allowlist_must_not_be_empty() {
    let mut manifest = manifest();
    manifest.allowed_origins.clear();

    assert!(matches!(
        validate(&manifest),
        Err(ManifestError::MissingAllowedOrigin)
    ));
}

fn valid_manifest_json() -> String {
    serde_json::json!({
        "name": expected_host_name(InstallChannel::Stable),
        "description": "Sniptale Native Host",
        "path": EXPECTED_PATH,
        "type": "stdio",
        "sniptale_native_host_manifest_version": "0.1.0",
        "allowed_origins": [EXPECTED_ORIGIN],
    })
    .to_string()
}
