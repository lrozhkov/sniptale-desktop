//! Local native-host response bridge for tests and non-Windows builds.

use std::io::{Read, Write};

use serde_json::Value;

use crate::{
    app::responses::{NativeHostSession, responses_for_message},
    ipc::{read_payload, write_payload},
    protocol::{
        EXTENSION_TO_HOST_MAX_MESSAGE_BYTES, HOST_TO_EXTENSION_MAX_MESSAGE_BYTES,
        parse_wire_message, parse_wire_message_with_limit,
    },
};

use super::NativeHostRunError;

pub(super) fn run_local_response_bridge(
    reader: &mut impl Read,
    writer: &mut impl Write,
) -> Result<(), NativeHostRunError> {
    #[cfg(test)]
    let mut session = NativeHostSession::default();
    #[cfg(not(test))]
    let mut session = NativeHostSession::system_clocked();
    while let Some(payload) = read_payload(reader, EXTENSION_TO_HOST_MAX_MESSAGE_BYTES)
        .map_err(NativeHostRunError::Frame)?
    {
        let message = parse_wire_message(&payload).map_err(NativeHostRunError::Protocol)?;
        let value: Value = serde_json::from_slice(&payload).map_err(NativeHostRunError::Json)?;
        for response in responses_for_message(&mut session, &message, &value) {
            let response_payload =
                serde_json::to_vec(&response).map_err(NativeHostRunError::Json)?;
            parse_wire_message_with_limit(&response_payload, HOST_TO_EXTENSION_MAX_MESSAGE_BYTES)
                .map_err(NativeHostRunError::Protocol)?;
            write_payload(
                writer,
                &response_payload,
                HOST_TO_EXTENSION_MAX_MESSAGE_BYTES,
            )
            .map_err(NativeHostRunError::Frame)?;
        }
    }
    Ok(())
}
