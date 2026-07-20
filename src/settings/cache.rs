//! Native technical cache schema and migration owner.

use std::{fs, path::PathBuf};

use serde::Deserialize;

use crate::protocol::CACHE_SCHEMA_VERSION;

const NATIVE_CACHE_FILE_NAME: &str = "native-cache.json";
const MAX_NATIVE_CACHE_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CacheSchemaStatus {
    Current,
    NeedsReset,
    RepairRequiredNewer { stored: u16, supported: u16 },
    RepairRequiredInvalid,
}

#[must_use]
pub(crate) const fn evaluate_cache_schema(stored_schema_version: Option<u16>) -> CacheSchemaStatus {
    match stored_schema_version {
        Some(stored) if stored == CACHE_SCHEMA_VERSION => CacheSchemaStatus::Current,
        Some(stored) if stored > CACHE_SCHEMA_VERSION => CacheSchemaStatus::RepairRequiredNewer {
            stored,
            supported: CACHE_SCHEMA_VERSION,
        },
        Some(_) | None => CacheSchemaStatus::NeedsReset,
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativeCacheHealth {
    pub(crate) schema_status: CacheSchemaStatus,
    pub(crate) rollback_protected: bool,
    pub(crate) operations_supported: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeCacheReadError {
    Io,
    Oversize,
    InvalidJson,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeTechnicalCache {
    schema_version: u16,
}

pub(crate) fn native_cache_health_from_path(path: Option<PathBuf>) -> NativeCacheHealth {
    native_cache_health(cache_schema_status_from_optional_file(path))
}

pub(crate) fn cache_schema_status_from_optional_file(path: Option<PathBuf>) -> CacheSchemaStatus {
    let Some(path) = path else {
        return CacheSchemaStatus::NeedsReset;
    };
    cache_schema_status_from_file(path)
}

pub(crate) const fn native_cache_health(schema_status: CacheSchemaStatus) -> NativeCacheHealth {
    NativeCacheHealth {
        schema_status,
        rollback_protected: rollback_protected(schema_status),
        operations_supported: cache_schema_supported(schema_status),
    }
}

pub(crate) const fn cache_schema_supported(status: CacheSchemaStatus) -> bool {
    matches!(
        status,
        CacheSchemaStatus::Current | CacheSchemaStatus::NeedsReset
    )
}

const fn rollback_protected(status: CacheSchemaStatus) -> bool {
    matches!(
        status,
        CacheSchemaStatus::Current | CacheSchemaStatus::NeedsReset
    )
}

pub(crate) fn cache_schema_status_from_file(path: PathBuf) -> CacheSchemaStatus {
    match read_native_cache_schema_version(path) {
        Ok(schema_version) => evaluate_cache_schema(Some(schema_version)),
        Err(NativeCacheReadError::Io) => CacheSchemaStatus::NeedsReset,
        Err(NativeCacheReadError::Oversize | NativeCacheReadError::InvalidJson) => {
            CacheSchemaStatus::RepairRequiredInvalid
        }
    }
}

fn read_native_cache_schema_version(path: PathBuf) -> Result<u16, NativeCacheReadError> {
    let payload = fs::read(path).map_err(|_| NativeCacheReadError::Io)?;
    parse_native_cache_schema_version(&payload)
}

pub(crate) fn parse_native_cache_schema_version(
    payload: &[u8],
) -> Result<u16, NativeCacheReadError> {
    if payload.len() > MAX_NATIVE_CACHE_BYTES {
        return Err(NativeCacheReadError::Oversize);
    }
    let cache: NativeTechnicalCache =
        serde_json::from_slice(payload).map_err(|_| NativeCacheReadError::InvalidJson)?;
    Ok(cache.schema_version)
}

pub(crate) const fn native_cache_file_name() -> &'static str {
    NATIVE_CACHE_FILE_NAME
}
