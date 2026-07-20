//! Protocol, settings schema, and native cache version constants.

pub(crate) const PROTOCOL_VERSION: u16 = 1;
pub(crate) const SETTINGS_SCHEMA_VERSION: u16 = 1;
pub(crate) const CACHE_SCHEMA_VERSION: u16 = 1;

#[cfg(test)]
pub(crate) const MIN_SUPPORTED_APP_VERSION: &str = "0.1.0";
pub(crate) const MIN_SUPPORTED_EXTENSION_VERSION: &str = "0.1.0";

pub(crate) const HOST_TO_EXTENSION_MAX_MESSAGE_BYTES: usize = 1_048_576;
#[cfg(test)]
pub(crate) const EXTENSION_TO_HOST_CHROMIUM_MAX_MESSAGE_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const EXTENSION_TO_HOST_CONTROL_MAX_MESSAGE_BYTES: usize = 1_048_576;
pub(crate) const EXTENSION_TO_HOST_MAX_MESSAGE_BYTES: usize =
    EXTENSION_TO_HOST_CONTROL_MAX_MESSAGE_BYTES;
pub(crate) const MESSAGE_TARGET_MAX_JSON_UTF8_BYTES: usize = 900 * 1024;
pub(crate) const CHUNK_RAW_BYTES: u64 = 512 * 1024;
pub(crate) const CHUNK_MAX_BASE64_CHARS: usize = 700_000;
pub(crate) const SCREENSHOT_MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;
pub(crate) const RECORDING_MAX_TOTAL_BYTES: u64 = 8 * 1024 * 1024 * 1024;

pub(crate) const MAX_ID_BYTES: usize = 128;
pub(crate) const MAX_FILENAME_BYTES: usize = 180;
pub(crate) const MAX_MICROPHONE_DEVICES: usize = 32;
pub(crate) const MAX_TRAY_ACTIONS: usize = 64;
pub(crate) const MAX_TRAY_SHORTCUT_PRIORITY_LABELS: usize = 64;
pub(crate) const MAX_UNAVAILABLE_REASONS: usize = 32;
pub(crate) const MAX_SETTINGS_WARNINGS: usize = 32;
pub(crate) const MAX_WARNING_FIELD_CHARS: usize = 80;
pub(crate) const MAX_WARNING_MESSAGE_CHARS: usize = 240;
