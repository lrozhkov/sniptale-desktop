use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fs,
    path::PathBuf,
};

use serde_json::{Value, json};

pub(super) use super::super::parse::{
    is_wire_message_contract, parse_wire_message, parse_wire_message_with_limit,
};
use super::super::{
    NativeAppErrorCode, OperationalErrorKind,
    errors::{ParseError, ParseErrorKind},
    versions::{
        CACHE_SCHEMA_VERSION, CHUNK_MAX_BASE64_CHARS, CHUNK_RAW_BYTES,
        EXTENSION_TO_HOST_CHROMIUM_MAX_MESSAGE_BYTES, EXTENSION_TO_HOST_CONTROL_MAX_MESSAGE_BYTES,
        EXTENSION_TO_HOST_MAX_MESSAGE_BYTES, HOST_TO_EXTENSION_MAX_MESSAGE_BYTES,
        MESSAGE_TARGET_MAX_JSON_UTF8_BYTES, MIN_SUPPORTED_APP_VERSION,
        MIN_SUPPORTED_EXTENSION_VERSION, PROTOCOL_VERSION, RECORDING_MAX_TOTAL_BYTES,
        SCREENSHOT_MAX_TOTAL_BYTES, SETTINGS_SCHEMA_VERSION,
    },
};

fn fixture_path(kind: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("contracts/native/fixtures")
        .join(kind)
        .join("contract-fixtures.json")
}

fn error_catalog_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("contracts/native/operation-error-catalog.json")
}

fn load_fixtures(kind: &str) -> Result<BTreeMap<String, Value>, Box<dyn Error>> {
    let text = fs::read_to_string(fixture_path(kind))?;
    Ok(serde_json::from_str(&text)?)
}

fn to_bytes(value: &Value) -> Result<Vec<u8>, Box<dyn Error>> {
    Ok(serde_json::to_vec(value)?)
}

fn positive_fixture(contract_id: &str) -> Result<Value, Box<dyn Error>> {
    let fixtures = load_fixtures("positive")?;
    fixtures
        .get(contract_id)
        .cloned()
        .ok_or_else(|| format!("missing fixture {contract_id}").into())
}

fn serialized_error_code(code: NativeAppErrorCode) -> Result<String, Box<dyn Error>> {
    let Value::String(value) = serde_json::to_value(code)? else {
        return Err("NativeAppErrorCode must serialize as string".into());
    };
    Ok(value)
}

mod fixtures;
mod media_limits;
mod metadata;
mod validation;
