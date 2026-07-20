//! Per-media encryption key wrapping for protected temp media files.

use serde::{Deserialize, Serialize};

pub(crate) const MEDIA_KEY_BYTES: usize = 32;
pub(crate) const MEDIA_NONCE_PREFIX_BYTES: usize = 16;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum MediaKeyProtection {
    CurrentUserDpapi,
    EquivalentCurrentUser,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProtectedMediaKey {
    pub(crate) algorithm: String,
    pub(crate) key_protection: MediaKeyProtection,
    pub(crate) protected_key_hex: String,
    pub(crate) nonce_prefix_hex: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProtectedMediaKeyError {
    Random,
    #[cfg(windows)]
    Protect,
    #[cfg(windows)]
    Unprotect,
    InvalidHex,
    InvalidKey,
    InvalidNonce,
    UnsupportedProtection,
}

pub(crate) fn new_protected_media_key(
    algorithm: &str,
) -> Result<(ProtectedMediaKey, [u8; MEDIA_KEY_BYTES]), ProtectedMediaKeyError> {
    let mut key = [0_u8; MEDIA_KEY_BYTES];
    let mut nonce_prefix = [0_u8; MEDIA_NONCE_PREFIX_BYTES];
    getrandom::fill(&mut key).map_err(|_| ProtectedMediaKeyError::Random)?;
    getrandom::fill(&mut nonce_prefix).map_err(|_| ProtectedMediaKeyError::Random)?;
    #[cfg(windows)]
    let protected = protect_key(&key)?;
    #[cfg(not(windows))]
    let protected = protect_key(&key);
    Ok((
        ProtectedMediaKey {
            algorithm: algorithm.to_owned(),
            key_protection: protected.protection,
            protected_key_hex: encode_hex(&protected.bytes),
            nonce_prefix_hex: encode_hex(&nonce_prefix),
        },
        key,
    ))
}

pub(crate) fn unprotect_media_key(
    protected: &ProtectedMediaKey,
    expected_algorithm: &str,
) -> Result<([u8; MEDIA_KEY_BYTES], [u8; MEDIA_NONCE_PREFIX_BYTES]), ProtectedMediaKeyError> {
    if protected.algorithm != expected_algorithm {
        return Err(ProtectedMediaKeyError::UnsupportedProtection);
    }
    let wrapped_key = decode_hex(&protected.protected_key_hex)?;
    let key = unprotect_key(protected.key_protection, &wrapped_key)?;
    let key: [u8; MEDIA_KEY_BYTES] = key
        .try_into()
        .map_err(|_| ProtectedMediaKeyError::InvalidKey)?;
    let nonce_prefix = decode_hex(&protected.nonce_prefix_hex)?;
    let nonce_prefix: [u8; MEDIA_NONCE_PREFIX_BYTES] = nonce_prefix
        .try_into()
        .map_err(|_| ProtectedMediaKeyError::InvalidNonce)?;
    Ok((key, nonce_prefix))
}

pub(crate) fn validate_protected_media_key(
    protected: &ProtectedMediaKey,
    expected_algorithm: &str,
) -> Result<(), ProtectedMediaKeyError> {
    if protected.algorithm != expected_algorithm {
        return Err(ProtectedMediaKeyError::UnsupportedProtection);
    }
    let protected_key = decode_hex(&protected.protected_key_hex)?;
    if protected_key.is_empty() {
        return Err(ProtectedMediaKeyError::InvalidKey);
    }
    let nonce_prefix = decode_hex(&protected.nonce_prefix_hex)?;
    if nonce_prefix.len() != MEDIA_NONCE_PREFIX_BYTES {
        return Err(ProtectedMediaKeyError::InvalidNonce);
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WrappedMediaKey {
    protection: MediaKeyProtection,
    bytes: Vec<u8>,
}

#[cfg(windows)]
fn protect_key(key: &[u8; MEDIA_KEY_BYTES]) -> Result<WrappedMediaKey, ProtectedMediaKeyError> {
    let protected = super::windows_dpapi::protect_current_user_media_key(key)
        .map_err(|_| ProtectedMediaKeyError::Protect)?;
    Ok(WrappedMediaKey {
        protection: MediaKeyProtection::CurrentUserDpapi,
        bytes: protected,
    })
}

#[cfg(not(windows))]
fn protect_key(key: &[u8; MEDIA_KEY_BYTES]) -> WrappedMediaKey {
    WrappedMediaKey {
        protection: MediaKeyProtection::EquivalentCurrentUser,
        bytes: key.to_vec(),
    }
}

#[cfg(windows)]
fn unprotect_key(
    protection: MediaKeyProtection,
    wrapped_key: &[u8],
) -> Result<Vec<u8>, ProtectedMediaKeyError> {
    match protection {
        MediaKeyProtection::CurrentUserDpapi => {
            super::windows_dpapi::unprotect_current_user_media_key(wrapped_key)
                .map_err(|_| ProtectedMediaKeyError::Unprotect)
        }
        MediaKeyProtection::EquivalentCurrentUser => {
            Err(ProtectedMediaKeyError::UnsupportedProtection)
        }
    }
}

#[cfg(not(windows))]
fn unprotect_key(
    protection: MediaKeyProtection,
    wrapped_key: &[u8],
) -> Result<Vec<u8>, ProtectedMediaKeyError> {
    match protection {
        MediaKeyProtection::EquivalentCurrentUser => Ok(wrapped_key.to_vec()),
        MediaKeyProtection::CurrentUserDpapi => Err(ProtectedMediaKeyError::UnsupportedProtection),
    }
}

fn decode_hex(value: &str) -> Result<Vec<u8>, ProtectedMediaKeyError> {
    let bytes = value.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err(ProtectedMediaKeyError::InvalidHex);
    }
    let mut decoded = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks_exact(2) {
        let high = hex_digit(pair[0])?;
        let low = hex_digit(pair[1])?;
        decoded.push((high << 4) | low);
    }
    Ok(decoded)
}

fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

fn hex_digit(byte: u8) -> Result<u8, ProtectedMediaKeyError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(ProtectedMediaKeyError::InvalidHex),
    }
}
