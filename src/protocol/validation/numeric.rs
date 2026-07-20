use serde_json::Value;

use crate::protocol::{
    errors::{ParseError, ParseErrorKind},
    versions::CHUNK_RAW_BYTES,
};

pub(crate) fn validate_numeric_field(
    key: &str,
    value: &Value,
    path: &str,
) -> Result<(), ParseError> {
    validate_audio_bitrate(key, value, path)?;
    validate_video_bitrate_override(key, value, path)?;
    validate_positive_finite_number(key, value, path)?;
    validate_max_duration_minutes(key, value, path)?;

    let Some(rule) = NumericRule::for_key(key) else {
        return Ok(());
    };

    let number = value.as_u64().ok_or_else(|| {
        ParseError::new(
            ParseErrorKind::InvalidNumber,
            path,
            "field must be an integer",
        )
    })?;

    if rule == NumericRule::Positive && number == 0 {
        return Err(ParseError::new(
            ParseErrorKind::InvalidNumber,
            path,
            "field must be positive",
        ));
    }

    if key == "chunkRawBytes" && number > CHUNK_RAW_BYTES {
        return Err(ParseError::new(
            ParseErrorKind::PayloadTooLarge,
            path,
            format!("chunk raw bytes {number} exceeds {CHUNK_RAW_BYTES}"),
        ));
    }

    Ok(())
}

fn validate_max_duration_minutes(key: &str, value: &Value, path: &str) -> Result<(), ParseError> {
    if key != "maxDurationMinutes" {
        return Ok(());
    }
    let number = value.as_f64().ok_or_else(|| {
        ParseError::new(
            ParseErrorKind::InvalidNumber,
            path,
            "max duration must be a finite number",
        )
    })?;
    if (1.0..=720.0).contains(&number) {
        Ok(())
    } else {
        Err(ParseError::new(
            ParseErrorKind::InvalidNumber,
            path,
            "max duration must be between 1 and 720 minutes",
        ))
    }
}

fn validate_positive_finite_number(key: &str, value: &Value, path: &str) -> Result<(), ParseError> {
    if !matches!(
        key,
        "effectiveAudioBitrateKbps" | "effectiveFps" | "effectiveVideoBitrateMbps"
    ) {
        return Ok(());
    }
    let number = value.as_f64().ok_or_else(|| {
        ParseError::new(
            ParseErrorKind::InvalidNumber,
            path,
            "field must be a finite number",
        )
    })?;
    if number > 0.0 {
        Ok(())
    } else {
        Err(ParseError::new(
            ParseErrorKind::InvalidNumber,
            path,
            "field must be positive",
        ))
    }
}

fn validate_audio_bitrate(key: &str, value: &Value, path: &str) -> Result<(), ParseError> {
    if key != "audioBitrateKbps" {
        return Ok(());
    }
    let number = value.as_u64().ok_or_else(|| {
        ParseError::new(
            ParseErrorKind::InvalidNumber,
            path,
            "audio bitrate must be an integer",
        )
    })?;
    if matches!(number, 96 | 128 | 160 | 192) {
        Ok(())
    } else {
        Err(ParseError::new(
            ParseErrorKind::InvalidNumber,
            path,
            "audio bitrate must be 96, 128, 160, or 192",
        ))
    }
}

fn validate_video_bitrate_override(key: &str, value: &Value, path: &str) -> Result<(), ParseError> {
    if key != "videoBitrateMbpsOverride" {
        return Ok(());
    }
    if value.is_null() {
        return Ok(());
    }
    let number = value.as_f64().ok_or_else(|| {
        ParseError::new(
            ParseErrorKind::InvalidNumber,
            path,
            "video bitrate override must be a number or null",
        )
    })?;
    if (2.0..=80.0).contains(&number) {
        Ok(())
    } else {
        Err(ParseError::new(
            ParseErrorKind::InvalidNumber,
            path,
            "video bitrate override must be between 2 and 80 Mbps",
        ))
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum NumericRule {
    NonNegative,
    Positive,
}

impl NumericRule {
    fn for_key(key: &str) -> Option<Self> {
        match key {
            "chunkIndex" | "chunkByteOffset" | "durationMs" | "bytesWritten" => {
                Some(Self::NonNegative)
            }
            "chunkRawBytes"
            | "totalBytes"
            | "width"
            | "height"
            | "chunkCount"
            | "capturedAtEpochMs"
            | "startedAtEpochMs"
            | "expiresAtEpochMs"
            | "generatedAtEpochMs"
            | "acceptedAtEpochMs"
            | "sentAtEpochMs"
            | "requestedAtEpochMs"
            | "occurredAtEpochMs"
            | "schemaVersion"
            | "settingsSchemaVersion"
            | "appCacheSchemaVersion"
            | "maxChunkBytes"
            | "maxRecordingBytes"
            | "maxScreenshotBytes"
            | "maxFps"
            | "maxWidth"
            | "maxHeight"
            | "fps" => Some(Self::Positive),
            _ => None,
        }
    }
}
