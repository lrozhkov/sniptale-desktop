use std::{
    error::Error,
    io::{Cursor, Error as IoError, Read, Write},
};

use crate::ipc::native_messaging::{
    FramePhase, NativeMessagingError, read_extension_message, read_payload, write_payload,
};
use crate::protocol::{
    EXTENSION_TO_HOST_CHROMIUM_MAX_MESSAGE_BYTES, EXTENSION_TO_HOST_MAX_MESSAGE_BYTES,
};
use proptest::prelude::*;

fn frame(payload: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let mut bytes = Vec::with_capacity(4 + payload.len());
    let length = u32::try_from(payload.len())?;
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

fn hello_payload() -> &'static [u8] {
    br#"{
        "type": "extension.hello",
        "protocolVersion": 1,
        "settingsSchemaVersion": 1,
        "extensionVersion": "0.1.0",
        "extensionId": "abcdefghijklmnopqrstuvwxzyabcdef",
        "minAppVersion": "0.1.0",
        "supportedProtocolVersions": [1],
        "supportedSettingsSchemaVersions": [1]
    }"#
}

#[test]
fn reads_valid_native_message_frame() -> Result<(), Box<dyn Error>> {
    let mut reader = Cursor::new(frame(hello_payload())?);
    let message = read_extension_message(&mut reader)?;
    assert!(message.is_some());
    Ok(())
}

#[test]
fn clean_eof_before_header_returns_none() -> Result<(), Box<dyn Error>> {
    let mut reader = Cursor::new(Vec::<u8>::new());
    let message = read_extension_message(&mut reader)?;
    assert!(message.is_none());
    Ok(())
}

#[test]
fn truncated_header_is_rejected() {
    let mut reader = Cursor::new(vec![1, 2]);
    let result = read_payload(&mut reader, 16);
    assert!(matches!(
        result,
        Err(NativeMessagingError::UnexpectedEof {
            phase: FramePhase::Header
        })
    ));
}

#[test]
fn truncated_body_is_rejected() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&8_u32.to_le_bytes());
    bytes.extend_from_slice(b"abc");
    let mut reader = Cursor::new(bytes);
    let result = read_payload(&mut reader, 16);
    assert!(matches!(
        result,
        Err(NativeMessagingError::UnexpectedEof {
            phase: FramePhase::Body
        })
    ));
}

#[test]
fn declared_oversize_is_rejected_before_payload_allocation() {
    let mut reader = Cursor::new(17_u32.to_le_bytes().to_vec());
    let result = read_payload(&mut reader, 16);
    assert!(matches!(
        result,
        Err(NativeMessagingError::MessageTooLarge {
            declared: 17,
            max: 16
        })
    ));
}

proptest! {
    #[test]
    fn native_message_declared_lengths_above_limit_never_read_body(declared in 17_u32..=4096) {
        let mut reader = HeaderOnlyReader::new(declared);
        let result = read_payload(&mut reader, 16);

        let expected_declared = declared as usize;
        match result {
            Err(NativeMessagingError::MessageTooLarge {
                declared: actual,
                max,
            }) => {
                prop_assert_eq!(actual, expected_declared);
                prop_assert_eq!(max, 16);
            }
            other => prop_assert!(false, "unexpected read result: {other:?}"),
        }
        prop_assert!(!reader.body_read_attempted);
    }
}

#[test]
fn chromium_sized_control_frame_is_rejected_before_body_read() -> Result<(), Box<dyn Error>> {
    let declared_len = u32::try_from(EXTENSION_TO_HOST_CHROMIUM_MAX_MESSAGE_BYTES)?;
    let mut reader = HeaderOnlyReader::new(declared_len);
    let result = read_payload(&mut reader, EXTENSION_TO_HOST_MAX_MESSAGE_BYTES);
    assert!(matches!(
        result,
        Err(NativeMessagingError::MessageTooLarge {
            declared: EXTENSION_TO_HOST_CHROMIUM_MAX_MESSAGE_BYTES,
            max: EXTENSION_TO_HOST_MAX_MESSAGE_BYTES
        })
    ));
    assert!(!reader.body_read_attempted);
    Ok(())
}

#[test]
fn malformed_json_frame_is_rejected_by_protocol_parser() -> Result<(), Box<dyn Error>> {
    let mut reader = Cursor::new(frame(br#"{"type":"extension.hello""#)?);
    let result = read_extension_message(&mut reader);
    assert!(matches!(result, Err(NativeMessagingError::Protocol(_))));
    Ok(())
}

#[test]
fn write_payload_emits_length_prefixed_frame() -> Result<(), Box<dyn Error>> {
    let mut output = Vec::new();
    write_payload(&mut output, b"abc", 16)?;
    assert_eq!(output, frame(b"abc")?);
    Ok(())
}

#[test]
fn write_payload_rejects_oversize() {
    let mut output = Vec::new();
    let result = write_payload(&mut output, b"abcdef", 5);
    assert!(matches!(
        result,
        Err(NativeMessagingError::MessageTooLarge {
            declared: 6,
            max: 5
        })
    ));
    assert_eq!(output.len(), 0);
}

struct HeaderOnlyReader {
    header: Cursor<Vec<u8>>,
    body_read_attempted: bool,
}

impl HeaderOnlyReader {
    fn new(declared_len: u32) -> Self {
        Self {
            header: Cursor::new(declared_len.to_le_bytes().to_vec()),
            body_read_attempted: false,
        }
    }
}

impl Read for HeaderOnlyReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let read = self.header.read(buffer)?;
        if read > 0 {
            return Ok(read);
        }
        self.body_read_attempted = true;
        Err(IoError::other("body read attempted"))
    }
}

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "closed",
        ))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn write_payload_propagates_disconnect() {
    let mut writer = FailingWriter;
    let result = write_payload(&mut writer, b"abc", 16);
    assert!(matches!(result, Err(NativeMessagingError::Io(_))));
}

#[test]
fn windows_pipe_close_errors_are_classified_as_closed_transport() {
    for code in [109, 232, -2_147_024_787, -2_147_024_664] {
        let error = NativeMessagingError::Io(std::io::Error::from_raw_os_error(code));
        assert!(
            error.is_closed_transport(),
            "raw os error {code} should be a closed transport"
        );
    }
}
