//! Per-install pipe secret record validation.

const INSTALL_ID_MIN_LEN: usize = 8;
const INSTALL_ID_MAX_LEN: usize = 64;
const USER_SID_MAX_LEN: usize = 128;
const PROTECTED_SECRET_MIN_BYTES: usize = 32;
pub(crate) const PIPE_SECRET_AUTH_ALGORITHM: &str = "pipe-auth-v2-hmac-sha256";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SecretProtection {
    CurrentUserDpapi,
    EquivalentCurrentUser,
    Plaintext,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PipeSecretRecord {
    pub(crate) install_id: String,
    pub(crate) current_user_sid: String,
    pub(crate) auth_algorithm: String,
    pub(crate) protection: SecretProtection,
    pub(crate) protected_secret_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PipeSecretRecordError {
    InvalidInstallId,
    WrongInstall,
    InvalidUserSid,
    WrongUser,
    UnsupportedAuthAlgorithm,
    MissingSecret,
    WeakSecret,
    UnprotectedSecret,
}

pub(crate) fn validate_pipe_secret_record(
    record: &PipeSecretRecord,
    expected_install_id: &str,
    expected_user_sid: &str,
) -> Result<(), PipeSecretRecordError> {
    validate_install_id(&record.install_id)?;
    validate_install_id(expected_install_id)?;
    validate_user_sid(&record.current_user_sid)?;
    validate_user_sid(expected_user_sid)?;
    if record.install_id != expected_install_id {
        return Err(PipeSecretRecordError::WrongInstall);
    }
    if record.current_user_sid != expected_user_sid {
        return Err(PipeSecretRecordError::WrongUser);
    }
    if record.auth_algorithm != PIPE_SECRET_AUTH_ALGORITHM {
        return Err(PipeSecretRecordError::UnsupportedAuthAlgorithm);
    }
    validate_secret_protection(record)
}

fn validate_secret_protection(record: &PipeSecretRecord) -> Result<(), PipeSecretRecordError> {
    if record.protected_secret_bytes == 0 {
        return Err(PipeSecretRecordError::MissingSecret);
    }
    if record.protected_secret_bytes < PROTECTED_SECRET_MIN_BYTES {
        return Err(PipeSecretRecordError::WeakSecret);
    }
    match record.protection {
        SecretProtection::CurrentUserDpapi | SecretProtection::EquivalentCurrentUser => Ok(()),
        SecretProtection::Plaintext => Err(PipeSecretRecordError::UnprotectedSecret),
    }
}

fn validate_install_id(install_id: &str) -> Result<(), PipeSecretRecordError> {
    if !(INSTALL_ID_MIN_LEN..=INSTALL_ID_MAX_LEN).contains(&install_id.len()) {
        return Err(PipeSecretRecordError::InvalidInstallId);
    }
    if install_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        Ok(())
    } else {
        Err(PipeSecretRecordError::InvalidInstallId)
    }
}

fn validate_user_sid(sid: &str) -> Result<(), PipeSecretRecordError> {
    if sid.len() > USER_SID_MAX_LEN {
        return Err(PipeSecretRecordError::InvalidUserSid);
    }
    let Some(rest) = sid.strip_prefix("S-1-") else {
        return Err(PipeSecretRecordError::InvalidUserSid);
    };
    if !rest.is_empty()
        && rest
            .bytes()
            .all(|byte| byte.is_ascii_digit() || byte == b'-')
    {
        Ok(())
    } else {
        Err(PipeSecretRecordError::InvalidUserSid)
    }
}
