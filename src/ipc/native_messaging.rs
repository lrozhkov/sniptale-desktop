//! Chromium Native Messaging length-prefixed stdio framing.

use std::{
    error::Error,
    fmt,
    io::{self, ErrorKind, Read, Write},
};

use crate::protocol::ParseError;
#[cfg(test)]
use crate::protocol::{EXTENSION_TO_HOST_MAX_MESSAGE_BYTES, WireMessage, parse_wire_message};

#[derive(Debug)]
pub(crate) enum NativeMessagingError {
    Io(io::Error),
    UnexpectedEof { phase: FramePhase },
    MessageTooLarge { declared: usize, max: usize },
    InvalidLength,
    Protocol(ParseError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FramePhase {
    Header,
    Body,
}

impl fmt::Display for NativeMessagingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "native messaging IO failed: {error}"),
            Self::UnexpectedEof { phase } => {
                write!(formatter, "native messaging stream ended during {phase:?}")
            }
            Self::MessageTooLarge { declared, max } => {
                write!(
                    formatter,
                    "native messaging frame declared {declared} bytes; max is {max}"
                )
            }
            Self::InvalidLength => write!(formatter, "native messaging frame length is invalid"),
            Self::Protocol(error) => {
                write!(formatter, "native messaging payload rejected: {error}")
            }
        }
    }
}

impl Error for NativeMessagingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Protocol(error) => Some(error),
            Self::UnexpectedEof { .. } | Self::MessageTooLarge { .. } | Self::InvalidLength => None,
        }
    }
}

impl From<io::Error> for NativeMessagingError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<ParseError> for NativeMessagingError {
    fn from(error: ParseError) -> Self {
        Self::Protocol(error)
    }
}

impl NativeMessagingError {
    #[cfg(any(windows, test))]
    pub(crate) fn is_closed_transport(&self) -> bool {
        let Self::Io(error) = self else {
            return false;
        };
        is_closed_transport_io_error(error)
    }
}

#[cfg(any(windows, test))]
pub(crate) fn is_closed_transport_io_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        ErrorKind::BrokenPipe | ErrorKind::ConnectionAborted | ErrorKind::ConnectionReset
    ) || matches!(
        error.raw_os_error(),
        // Windows named pipes report peer shutdown as ERROR_BROKEN_PIPE or ERROR_NO_DATA.
        // The windows crate can surface either the plain Win32 code or HRESULT_FROM_WIN32(code).
        Some(109 | 232 | -2_147_024_787 | -2_147_024_664)
    )
}

#[cfg(test)]
pub(crate) fn read_extension_message<R: Read>(
    reader: &mut R,
) -> Result<Option<WireMessage>, NativeMessagingError> {
    let Some(payload) = read_payload(reader, EXTENSION_TO_HOST_MAX_MESSAGE_BYTES)? else {
        return Ok(None);
    };
    Ok(Some(parse_wire_message(&payload)?))
}

pub(crate) fn read_payload<R: Read>(
    reader: &mut R,
    max_message_bytes: usize,
) -> Result<Option<Vec<u8>>, NativeMessagingError> {
    let Some(length) = read_frame_length(reader)? else {
        return Ok(None);
    };
    if length > max_message_bytes {
        return Err(NativeMessagingError::MessageTooLarge {
            declared: length,
            max: max_message_bytes,
        });
    }

    let mut payload = vec![0_u8; length];
    read_exact_or_eof(reader, &mut payload, FramePhase::Body)?;
    Ok(Some(payload))
}

pub(crate) fn write_payload<W: Write>(
    writer: &mut W,
    payload: &[u8],
    max_message_bytes: usize,
) -> Result<(), NativeMessagingError> {
    if payload.len() > max_message_bytes {
        return Err(NativeMessagingError::MessageTooLarge {
            declared: payload.len(),
            max: max_message_bytes,
        });
    }

    let length = u32::try_from(payload.len()).map_err(|_| NativeMessagingError::InvalidLength)?;
    writer.write_all(&length.to_le_bytes())?;
    writer.write_all(payload)?;
    writer.flush()?;
    Ok(())
}

fn read_frame_length<R: Read>(reader: &mut R) -> Result<Option<usize>, NativeMessagingError> {
    let mut header = [0_u8; 4];
    let mut read = 0_usize;

    while read < header.len() {
        match reader.read(&mut header[read..]) {
            Ok(0) if read == 0 => return Ok(None),
            Ok(0) => {
                return Err(NativeMessagingError::UnexpectedEof {
                    phase: FramePhase::Header,
                });
            }
            Ok(count) => read += count,
            Err(error) if error.kind() == ErrorKind::Interrupted => {}
            Err(error) => return Err(error.into()),
        }
    }

    Ok(Some(u32::from_le_bytes(header) as usize))
}

fn read_exact_or_eof<R: Read>(
    reader: &mut R,
    buffer: &mut [u8],
    phase: FramePhase,
) -> Result<(), NativeMessagingError> {
    reader
        .read_exact(buffer)
        .map_err(|error| match error.kind() {
            ErrorKind::UnexpectedEof => NativeMessagingError::UnexpectedEof { phase },
            _ => NativeMessagingError::Io(error),
        })
}
