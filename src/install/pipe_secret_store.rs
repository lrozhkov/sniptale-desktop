//! Persisted per-install pipe secret file parser.

mod provision;
#[cfg(windows)]
pub(crate) use provision::initialize_current_user_pipe_secret;

use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
};

use serde::Deserialize;

use super::pipe_secret::{
    PipeSecretRecord, PipeSecretRecordError, SecretProtection, validate_pipe_secret_record,
};
#[cfg(windows)]
use super::windows::{
    dpapi::{DpapiSecretError, unprotect_current_user_secret},
    user::{WindowsUserError, current_user_sid},
};

const PIPE_SECRET_FILE_RELATIVE_PATH: &[&str] = &["Sniptale", "NativeAgent", "pipe-secret.json"];
const MAX_PIPE_SECRET_FILE_BYTES: usize = 16 * 1024;

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct ProtectedPipeSecret {
    pub(crate) install_id: String,
    pub(crate) current_user_sid: String,
    pub(crate) auth_algorithm: String,
    pub(crate) protection: SecretProtection,
    pub(crate) protected_secret: Vec<u8>,
}

impl fmt::Debug for ProtectedPipeSecret {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProtectedPipeSecret")
            .field("install_id", &self.install_id)
            .field("current_user_sid", &self.current_user_sid)
            .field("auth_algorithm", &self.auth_algorithm)
            .field("protection", &self.protection)
            .field("protected_secret_bytes", &self.protected_secret.len())
            .finish()
    }
}

#[derive(Debug)]
pub(crate) enum PipeSecretFileError {
    Oversized,
    Json(serde_json::Error),
    UnknownProtection,
    InvalidHex,
    Record(PipeSecretRecordError),
}

impl fmt::Display for PipeSecretFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Oversized => formatter.write_str("pipe secret file exceeds its size limit"),
            Self::Json(error) => write!(formatter, "pipe secret JSON rejected: {error}"),
            Self::UnknownProtection => formatter.write_str("pipe secret protection is unknown"),
            Self::InvalidHex => formatter.write_str("pipe secret hex is invalid"),
            Self::Record(error) => write!(formatter, "pipe secret record rejected: {error:?}"),
        }
    }
}

impl Error for PipeSecretFileError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            Self::Oversized | Self::UnknownProtection | Self::InvalidHex | Self::Record(_) => None,
        }
    }
}

#[cfg(windows)]
#[derive(Debug)]
pub(crate) enum PipeSecretLoadError {
    MissingLocalAppData,
    CurrentUser(WindowsUserError),
    Io(std::io::Error),
    File(PipeSecretFileError),
    UnsupportedProtection,
    Dpapi(DpapiSecretError),
    InvalidSecretUtf8,
}

#[cfg(windows)]
impl fmt::Display for PipeSecretLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingLocalAppData => formatter.write_str("LOCALAPPDATA is unavailable"),
            Self::CurrentUser(error) => write!(formatter, "current user SID failed: {error:?}"),
            Self::Io(error) => write!(formatter, "pipe secret file read failed: {error}"),
            Self::File(error) => write!(formatter, "pipe secret file rejected: {error}"),
            Self::UnsupportedProtection => {
                formatter.write_str("pipe secret protection is unsupported")
            }
            Self::Dpapi(error) => write!(formatter, "pipe secret DPAPI unwrap failed: {error:?}"),
            Self::InvalidSecretUtf8 => formatter.write_str("pipe secret material is not UTF-8"),
        }
    }
}

#[cfg(windows)]
impl Error for PipeSecretLoadError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::File(error) => Some(error),
            Self::MissingLocalAppData
            | Self::CurrentUser(_)
            | Self::UnsupportedProtection
            | Self::Dpapi(_)
            | Self::InvalidSecretUtf8 => None,
        }
    }
}

#[cfg(windows)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct UnprotectedPipeSecret {
    pub(crate) install_id: String,
    pub(crate) current_user_sid: String,
    pub(crate) secret_material: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PipeSecretFileDto {
    install_id: String,
    current_user_sid: String,
    auth_algorithm: String,
    protection: String,
    protected_secret_hex: String,
}

pub(crate) fn parse_pipe_secret_file(
    payload: &[u8],
    expected_install_id: &str,
    expected_user_sid: &str,
) -> Result<ProtectedPipeSecret, PipeSecretFileError> {
    if payload.len() > MAX_PIPE_SECRET_FILE_BYTES {
        return Err(PipeSecretFileError::Oversized);
    }
    let dto: PipeSecretFileDto =
        serde_json::from_slice(payload).map_err(PipeSecretFileError::Json)?;
    let protection = parse_protection(&dto.protection)?;
    let protected_secret = decode_hex(&dto.protected_secret_hex)?;
    let record = PipeSecretRecord {
        install_id: dto.install_id.clone(),
        current_user_sid: dto.current_user_sid.clone(),
        auth_algorithm: dto.auth_algorithm.clone(),
        protection,
        protected_secret_bytes: protected_secret.len(),
    };
    validate_pipe_secret_record(&record, expected_install_id, expected_user_sid)
        .map_err(PipeSecretFileError::Record)?;
    Ok(ProtectedPipeSecret {
        install_id: dto.install_id,
        current_user_sid: dto.current_user_sid,
        auth_algorithm: dto.auth_algorithm,
        protection,
        protected_secret,
    })
}

pub(crate) fn pipe_secret_file_path(local_app_data: &Path) -> PathBuf {
    let mut path = local_app_data.to_path_buf();
    for segment in PIPE_SECRET_FILE_RELATIVE_PATH {
        path.push(segment);
    }
    path
}

#[cfg(windows)]
pub(crate) fn load_current_user_pipe_secret(
    expected_install_id: &str,
) -> Result<UnprotectedPipeSecret, PipeSecretLoadError> {
    let current_user_sid = current_user_sid().map_err(PipeSecretLoadError::CurrentUser)?;
    let local_app_data =
        std::env::var_os("LOCALAPPDATA").ok_or(PipeSecretLoadError::MissingLocalAppData)?;
    let path = pipe_secret_file_path(Path::new(&local_app_data));
    use std::io::Read;
    let mut payload = Vec::new();
    std::fs::File::open(path)
        .map_err(PipeSecretLoadError::Io)?
        .take((MAX_PIPE_SECRET_FILE_BYTES + 1) as u64)
        .read_to_end(&mut payload)
        .map_err(PipeSecretLoadError::Io)?;
    let protected = parse_pipe_secret_file(&payload, expected_install_id, &current_user_sid)
        .map_err(PipeSecretLoadError::File)?;
    if !pipe_secret_load_protection_supported(protected.protection) {
        return Err(PipeSecretLoadError::UnsupportedProtection);
    }
    let secret = unprotect_current_user_secret(&protected.protected_secret)
        .map_err(PipeSecretLoadError::Dpapi)?;
    let secret_material =
        String::from_utf8(secret).map_err(|_| PipeSecretLoadError::InvalidSecretUtf8)?;
    Ok(UnprotectedPipeSecret {
        install_id: protected.install_id,
        current_user_sid: protected.current_user_sid,
        secret_material,
    })
}

pub(crate) const fn pipe_secret_load_protection_supported(protection: SecretProtection) -> bool {
    matches!(protection, SecretProtection::CurrentUserDpapi)
}

fn parse_protection(value: &str) -> Result<SecretProtection, PipeSecretFileError> {
    match value {
        "current-user-dpapi" => Ok(SecretProtection::CurrentUserDpapi),
        "equivalent-current-user" => Ok(SecretProtection::EquivalentCurrentUser),
        "plaintext" => Ok(SecretProtection::Plaintext),
        _ => Err(PipeSecretFileError::UnknownProtection),
    }
}

fn decode_hex(value: &str) -> Result<Vec<u8>, PipeSecretFileError> {
    let bytes = value.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err(PipeSecretFileError::InvalidHex);
    }
    let mut decoded = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let high = hex_digit(pair[0])?;
        let low = hex_digit(pair[1])?;
        decoded.push((high << 4) | low);
    }
    Ok(decoded)
}

fn hex_digit(byte: u8) -> Result<u8, PipeSecretFileError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(PipeSecretFileError::InvalidHex),
    }
}
