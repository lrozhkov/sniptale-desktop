use std::path::Path;

use super::super::{
    pipe_secret::{
        PIPE_SECRET_AUTH_ALGORITHM, PipeSecretRecord, PipeSecretRecordError, SecretProtection,
        validate_pipe_secret_record,
    },
    pipe_secret_store::pipe_secret_load_protection_supported,
    pipe_secret_store::{PipeSecretFileError, parse_pipe_secret_file, pipe_secret_file_path},
    registry::{InstallChannel, expected_install_id},
};

const INSTALL_ID: &str = "install-stable";
const USER_SID: &str = "S-1-5-21-1000";

fn record() -> PipeSecretRecord {
    PipeSecretRecord {
        install_id: INSTALL_ID.to_owned(),
        current_user_sid: USER_SID.to_owned(),
        auth_algorithm: PIPE_SECRET_AUTH_ALGORITHM.to_owned(),
        protection: SecretProtection::CurrentUserDpapi,
        protected_secret_bytes: 64,
    }
}

#[test]
fn protected_current_user_pipe_secret_record_is_valid() -> Result<(), PipeSecretRecordError> {
    validate_pipe_secret_record(&record(), INSTALL_ID, USER_SID)
}

#[test]
fn pipe_secret_record_is_bound_to_install_and_user() {
    let mut wrong_install = record();
    wrong_install.install_id = "install-beta".to_owned();
    assert_eq!(
        validate_pipe_secret_record(&wrong_install, INSTALL_ID, USER_SID),
        Err(PipeSecretRecordError::WrongInstall)
    );

    let mut wrong_user = record();
    wrong_user.current_user_sid = "S-1-5-21-2000".to_owned();
    assert_eq!(
        validate_pipe_secret_record(&wrong_user, INSTALL_ID, USER_SID),
        Err(PipeSecretRecordError::WrongUser)
    );
}

#[test]
fn pipe_secret_record_rejects_path_or_sid_injection() {
    let mut bad_install = record();
    bad_install.install_id = "install\\escape".to_owned();
    assert_eq!(
        validate_pipe_secret_record(&bad_install, INSTALL_ID, USER_SID),
        Err(PipeSecretRecordError::InvalidInstallId)
    );

    let mut bad_sid = record();
    bad_sid.current_user_sid = "S-1-5-21-admin".to_owned();
    assert_eq!(
        validate_pipe_secret_record(&bad_sid, INSTALL_ID, USER_SID),
        Err(PipeSecretRecordError::InvalidUserSid)
    );
}

#[test]
fn pipe_secret_record_fails_closed_without_current_user_protection() {
    let mut missing = record();
    missing.protected_secret_bytes = 0;
    assert_eq!(
        validate_pipe_secret_record(&missing, INSTALL_ID, USER_SID),
        Err(PipeSecretRecordError::MissingSecret)
    );

    let mut weak = record();
    weak.protected_secret_bytes = 16;
    assert_eq!(
        validate_pipe_secret_record(&weak, INSTALL_ID, USER_SID),
        Err(PipeSecretRecordError::WeakSecret)
    );

    let mut plaintext = record();
    plaintext.protection = SecretProtection::Plaintext;
    assert_eq!(
        validate_pipe_secret_record(&plaintext, INSTALL_ID, USER_SID),
        Err(PipeSecretRecordError::UnprotectedSecret)
    );
}

#[test]
fn equivalent_current_user_secret_store_is_allowed_for_future_platforms()
-> Result<(), PipeSecretRecordError> {
    let mut equivalent = record();
    equivalent.protection = SecretProtection::EquivalentCurrentUser;

    validate_pipe_secret_record(&equivalent, INSTALL_ID, USER_SID)
}

#[test]
fn install_ids_are_channel_scoped_for_pipe_names() {
    assert_eq!(
        expected_install_id(InstallChannel::Stable),
        "install-stable"
    );
    assert_eq!(expected_install_id(InstallChannel::Beta), "install-beta");
    assert_eq!(expected_install_id(InstallChannel::Dev), "install-dev");
}

#[test]
fn pipe_secret_file_path_lives_under_local_app_data() {
    let path = pipe_secret_file_path(Path::new(r"C:\Users\Alice\AppData\Local"));

    assert!(path.ends_with(Path::new("Sniptale/NativeAgent/pipe-secret.json")));
}

#[test]
fn pipe_secret_file_parser_accepts_bound_current_user_secret() -> Result<(), PipeSecretFileError> {
    let parsed = parse_pipe_secret_file(
        file_payload(INSTALL_ID, USER_SID).as_bytes(),
        INSTALL_ID,
        USER_SID,
    )?;

    assert_eq!(parsed.install_id, INSTALL_ID);
    assert_eq!(parsed.current_user_sid, USER_SID);
    assert_eq!(parsed.auth_algorithm, PIPE_SECRET_AUTH_ALGORITHM);
    assert_eq!(parsed.protection, SecretProtection::CurrentUserDpapi);
    assert_eq!(parsed.protected_secret.len(), 32);
    assert!(!format!("{parsed:?}").contains("abab"));
    Ok(())
}

#[test]
fn pipe_secret_file_parser_rejects_wrong_install_or_user() {
    assert!(matches!(
        parse_pipe_secret_file(
            file_payload("install-beta", USER_SID).as_bytes(),
            INSTALL_ID,
            USER_SID
        ),
        Err(PipeSecretFileError::Record(
            PipeSecretRecordError::WrongInstall
        ))
    ));
    assert!(matches!(
        parse_pipe_secret_file(
            file_payload(INSTALL_ID, "S-1-5-21-2000").as_bytes(),
            INSTALL_ID,
            USER_SID
        ),
        Err(PipeSecretFileError::Record(
            PipeSecretRecordError::WrongUser
        ))
    ));
}

#[test]
fn pipe_secret_file_parser_rejects_untrusted_file_shapes() {
    assert!(matches!(
        parse_pipe_secret_file(
            br#"{"installId":"install-stable","currentUserSid":"S-1-5-21-1000""#,
            INSTALL_ID,
            USER_SID
        ),
        Err(PipeSecretFileError::Json(_))
    ));
    assert!(matches!(
        parse_pipe_secret_file(
            file_payload_with("current-user-dpapi", "abc").as_bytes(),
            INSTALL_ID,
            USER_SID
        ),
        Err(PipeSecretFileError::InvalidHex)
    ));
    assert!(matches!(
        parse_pipe_secret_file(
            file_payload_with("machine-wide", &"ab".repeat(32)).as_bytes(),
            INSTALL_ID,
            USER_SID
        ),
        Err(PipeSecretFileError::UnknownProtection)
    ));
    assert!(matches!(
        parse_pipe_secret_file(
            file_payload_with("plaintext", &"ab".repeat(32)).as_bytes(),
            INSTALL_ID,
            USER_SID
        ),
        Err(PipeSecretFileError::Record(
            PipeSecretRecordError::UnprotectedSecret
        ))
    ));
    assert!(matches!(
        parse_pipe_secret_file(
            file_payload_with_auth("pipe-auth-v1-sha256").as_bytes(),
            INSTALL_ID,
            USER_SID
        ),
        Err(PipeSecretFileError::Record(
            PipeSecretRecordError::UnsupportedAuthAlgorithm
        ))
    ));
    assert!(matches!(
        parse_pipe_secret_file(
            r#"{
                "installId": "install-stable",
                "currentUserSid": "S-1-5-21-1000",
                "protection": "current-user-dpapi",
                "protectedSecretHex": "abababababababababababababababababababababababababababababababab"
            }"#
            .as_bytes(),
            INSTALL_ID,
            USER_SID
        ),
        Err(PipeSecretFileError::Json(_))
    ));
}

#[test]
fn pipe_secret_load_only_allows_current_user_dpapi_before_unwrap() {
    assert!(pipe_secret_load_protection_supported(
        SecretProtection::CurrentUserDpapi
    ));
    assert!(!pipe_secret_load_protection_supported(
        SecretProtection::EquivalentCurrentUser
    ));
    assert!(!pipe_secret_load_protection_supported(
        SecretProtection::Plaintext
    ));
}

fn file_payload(install_id: &str, current_user_sid: &str) -> String {
    format!(
        r#"{{
            "installId": "{install_id}",
            "currentUserSid": "{current_user_sid}",
            "authAlgorithm": "{PIPE_SECRET_AUTH_ALGORITHM}",
            "protection": "current-user-dpapi",
            "protectedSecretHex": "{}"
        }}"#,
        "ab".repeat(32)
    )
}

fn file_payload_with(protection: &str, protected_secret_hex: &str) -> String {
    format!(
        r#"{{
            "installId": "{INSTALL_ID}",
            "currentUserSid": "{USER_SID}",
            "authAlgorithm": "{PIPE_SECRET_AUTH_ALGORITHM}",
            "protection": "{protection}",
            "protectedSecretHex": "{protected_secret_hex}"
        }}"#
    )
}

fn file_payload_with_auth(auth_algorithm: &str) -> String {
    format!(
        r#"{{
            "installId": "{INSTALL_ID}",
            "currentUserSid": "{USER_SID}",
            "authAlgorithm": "{auth_algorithm}",
            "protection": "current-user-dpapi",
            "protectedSecretHex": "{}"
        }}"#,
        "ab".repeat(32)
    )
}
