//! Typed protocol and native-app error codes.

use std::{error::Error, fmt};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ParseErrorKind {
    MalformedJson,
    MissingType,
    UnknownType,
    UnsupportedVersion,
    OversizedMessage,
    InvalidIdentifier,
    InvalidFilename,
    InvalidHash,
    InvalidBase64,
    InvalidNumber,
    ArrayTooLong,
    PayloadTooLarge,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ParseError {
    kind: ParseErrorKind,
    field: Option<String>,
    detail: String,
}

impl ParseError {
    pub(crate) fn new(
        kind: ParseErrorKind,
        field: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            field: Some(field.into()),
            detail: detail.into(),
        }
    }

    pub(crate) fn without_field(kind: ParseErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            field: None,
            detail: detail.into(),
        }
    }

    #[cfg(test)]
    pub(crate) const fn kind(&self) -> &ParseErrorKind {
        &self.kind
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.field {
            Some(field) => write!(formatter, "{:?} at {field}: {}", self.kind, self.detail),
            None => write!(formatter, "{:?}: {}", self.kind, self.detail),
        }
    }
}

impl Error for ParseError {}
