//! Transfer-owned media identifiers, filenames, hashes, and safe leaf paths.

use std::path::{Path, PathBuf};

use super::hashing::sha256_hex;

const MAX_MEDIA_ID_BYTES: usize = 96;
const MAX_MEDIA_FILENAME_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TransferIdentityError {
    EmptyId,
    InvalidId,
    InvalidFilename,
    InvalidHash,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TransferMediaFileKind {
    Png,
    Mp4,
    Json,
    JsonTemp,
}

impl TransferMediaFileKind {
    const fn suffix(self) -> &'static str {
        match self {
            Self::Png => ".png",
            Self::Mp4 => ".mp4",
            Self::Json => ".json",
            Self::JsonTemp => ".json.tmp",
        }
    }

    const fn is_user_visible_media(self) -> bool {
        matches!(self, Self::Png | Self::Mp4)
    }
}

pub(crate) fn derived_media_id(prefix: &str, seed: &str) -> String {
    format!("{prefix}-{}", sha256_hex(seed.as_bytes()))
}

pub(crate) fn validate_media_id(media_id: &str) -> Result<(), TransferIdentityError> {
    if media_id.is_empty() {
        return Err(TransferIdentityError::EmptyId);
    }
    if media_id.len() > MAX_MEDIA_ID_BYTES || has_path_prefix_or_separator(media_id) {
        return Err(TransferIdentityError::InvalidId);
    }
    if !media_id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(TransferIdentityError::InvalidId);
    }
    if media_id
        .split('.')
        .any(|segment| segment.is_empty() || is_reserved_device_name(segment))
    {
        return Err(TransferIdentityError::InvalidId);
    }
    Ok(())
}

pub(crate) fn validate_media_filename(
    filename: &str,
    kind: TransferMediaFileKind,
) -> Result<(), TransferIdentityError> {
    if !kind.is_user_visible_media()
        || filename.is_empty()
        || filename.len() > MAX_MEDIA_FILENAME_BYTES
    {
        return Err(TransferIdentityError::InvalidFilename);
    }
    let Some(stem) = filename.strip_suffix(kind.suffix()) else {
        return Err(TransferIdentityError::InvalidFilename);
    };
    validate_media_id(stem).map_err(|_| TransferIdentityError::InvalidFilename)
}

pub(crate) fn validate_sha256_hex(sha256: &str) -> Result<(), TransferIdentityError> {
    let valid = sha256.len() == 64
        && sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase());
    if valid {
        Ok(())
    } else {
        Err(TransferIdentityError::InvalidHash)
    }
}

pub(crate) fn media_filename(
    media_id: &str,
    kind: TransferMediaFileKind,
) -> Result<String, TransferIdentityError> {
    if !kind.is_user_visible_media() {
        return Err(TransferIdentityError::InvalidFilename);
    }
    media_leaf_name(media_id, kind)
}

pub(crate) fn media_path(
    root: &Path,
    media_id: &str,
    kind: TransferMediaFileKind,
) -> Result<PathBuf, TransferIdentityError> {
    let leaf = media_leaf_name(media_id, kind)?;
    Ok(root.join(leaf))
}

fn media_leaf_name(
    media_id: &str,
    kind: TransferMediaFileKind,
) -> Result<String, TransferIdentityError> {
    validate_media_id(media_id)?;
    Ok(format!("{media_id}{}", kind.suffix()))
}

fn has_path_prefix_or_separator(value: &str) -> bool {
    value.starts_with(['/', '\\'])
        || value.contains(['/', '\\', ':'])
        || value.as_bytes().get(1).is_some_and(|byte| *byte == b':')
}

fn is_reserved_device_name(segment: &str) -> bool {
    matches!(
        segment.to_ascii_lowercase().as_str(),
        "con"
            | "prn"
            | "aux"
            | "nul"
            | "com1"
            | "com2"
            | "com3"
            | "com4"
            | "com5"
            | "com6"
            | "com7"
            | "com8"
            | "com9"
            | "lpt1"
            | "lpt2"
            | "lpt3"
            | "lpt4"
            | "lpt5"
            | "lpt6"
            | "lpt7"
            | "lpt8"
            | "lpt9"
    )
}
