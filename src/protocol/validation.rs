mod fields;
mod numeric;
mod warnings;

use serde_json::Value;

use super::errors::ParseError;

pub(crate) fn validate_named_value(key: &str, value: &Value, path: &str) -> Result<(), ParseError> {
    fields::validate_identifier_field(key, value, path)?;
    fields::validate_filename_field(key, value, path)?;
    fields::validate_hash_field(key, value, path)?;
    fields::validate_base64_field(key, value, path)?;
    fields::validate_digit_string_field(key, value, path)?;
    numeric::validate_numeric_field(key, value, path)?;
    fields::validate_array_limit(key, value, path)?;
    warnings::validate_settings_warnings(key, value, path)
}
