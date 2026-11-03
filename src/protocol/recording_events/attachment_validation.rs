//! Closed event envelopes allow overlapping source-time ranges and keep independent receipts.

use super::{
    AppRecordingEventsChunkMessage, AppRecordingEventsManifestMessage, EventAlgorithm,
    EventContractError, EventDescriptor, EventEncoding, EventReceipt,
    NativeEventAttachmentCapabilities,
};
use crate::protocol::{
    WireMessage,
    trajectory::{MAX_SAFE_INTEGER, valid_trajectory_hash, valid_trajectory_id},
};

pub(crate) fn validate_event_attachment_message(
    message: &WireMessage,
) -> Result<(), EventContractError> {
    match message {
        WireMessage::ExtensionRecordingEventsManifestRequest(value) => {
            identity(
                &value.recording_id,
                &value.event_stream_id,
                Some(&value.request_id),
            )?;
            require(
                value.next_sequence <= MAX_SAFE_INTEGER
                    && valid_trajectory_hash(&value.prefix_sha256),
            )
        }
        WireMessage::ExtensionRecordingEventsChunkRequest(value) => {
            identity(
                &value.recording_id,
                &value.event_stream_id,
                Some(&value.request_id),
            )?;
            require(value.sequence <= MAX_SAFE_INTEGER && value.chunk_index < 2)
        }
        WireMessage::ExtensionRecordingEventsAck(value) => {
            identity(
                &value.recording_id,
                &value.event_stream_id,
                Some(&value.request_id),
            )?;
            receipt(&value.receipt, &value.event_stream_id)
        }
        WireMessage::AppRecordingEventsManifest(value) => manifest(value),
        WireMessage::AppRecordingEventsChunk(value) => chunk(value),
        WireMessage::AppRecordingEventsAvailable(value) => {
            identity(&value.recording_id, &value.event_stream_id, None)?;
            value.descriptor.validate()?;
            require(
                value.event_stream_id == value.descriptor.event_stream_id
                    && valid_trajectory_hash(&value.video_sha256),
            )
        }
        WireMessage::AppRecordingStopped(value) => {
            if let Some(descriptor) = &value.recording_events {
                descriptor.validate()?;
            }
            Ok(())
        }
        WireMessage::AppHello(value) => {
            if let Some(capability) = &value.capabilities.event_attachments {
                require(*capability == NativeEventAttachmentCapabilities::v1())?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub(super) fn id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn identity(
    recording: &str,
    source: &str,
    request: Option<&str>,
) -> Result<(), EventContractError> {
    require(id(recording) && valid_trajectory_id(source) && request.is_none_or(id))
}

fn require(valid: bool) -> Result<(), EventContractError> {
    if valid {
        Ok(())
    } else {
        Err(EventContractError::Invalid)
    }
}

fn receipt(value: &EventReceipt, source: &str) -> Result<(), EventContractError> {
    require(value.version == 1 && value.durable)?;
    EventDescriptor {
        format: value.format,
        format_version: 1,
        algorithm: EventAlgorithm::LosslessV1,
        encoding: EventEncoding::JsonUtf8V1,
        event_stream_id: source.to_owned(),
        state: value.state,
        finalized: true,
        total_blocks: value.total_blocks,
        total_records: value.total_records,
        total_bytes: value.total_bytes,
        stream_sha256: value.stream_sha256.clone(),
    }
    .validate()
}

fn manifest(value: &AppRecordingEventsManifestMessage) -> Result<(), EventContractError> {
    identity(
        &value.recording_id,
        &value.event_stream_id,
        Some(&value.request_id),
    )?;
    value.descriptor.validate()?;
    require(
        value.event_stream_id == value.descriptor.event_stream_id
            && value.entries.len() <= 32
            && value.from_sequence <= value.descriptor.total_blocks,
    )?;
    let end = value
        .from_sequence
        .checked_add(u64::try_from(value.entries.len()).map_err(|_| EventContractError::Invalid)?)
        .ok_or(EventContractError::Invalid)?;
    require(
        end <= value.descriptor.total_blocks
            && value.next_sequence == (end < value.descriptor.total_blocks).then_some(end)
            && (end == value.descriptor.total_blocks || !value.entries.is_empty()),
    )?;
    for (index, entry) in value.entries.iter().enumerate() {
        require(
            entry.sequence
                == value.from_sequence
                    + u64::try_from(index).map_err(|_| EventContractError::Invalid)?
                && entry.start_time_us <= entry.end_time_us
                && entry.end_time_us <= MAX_SAFE_INTEGER
                && (1..=512).contains(&entry.record_count)
                && (1..=65536).contains(&entry.byte_length)
                && valid_trajectory_hash(&entry.block_sha256)
                && valid_trajectory_hash(&entry.prefix_sha256),
        )?;
    }
    if end == value.descriptor.total_blocks
        && let Some(last) = value.entries.last()
    {
        require(last.prefix_sha256 == value.descriptor.stream_sha256)?;
    }
    Ok(())
}

fn chunk(value: &AppRecordingEventsChunkMessage) -> Result<(), EventContractError> {
    identity(
        &value.recording_id,
        &value.event_stream_id,
        Some(&value.request_id),
    )?;
    require(
        value.sequence <= MAX_SAFE_INTEGER
            && (1..=65536).contains(&value.block_bytes)
            && u32::from(value.chunk_count) == value.block_bytes.div_ceil(32768)
            && value.chunk_index < value.chunk_count
            && valid_trajectory_hash(&value.block_sha256)
            && valid_trajectory_hash(&value.chunk_sha256),
    )?;
    let offset = u32::from(value.chunk_index) * 32768;
    let expected = (value.block_bytes - offset).min(32768);
    let padding = value
        .base64
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'=')
        .count();
    let raw_length = (value.base64.len() / 4 * 3).checked_sub(padding);
    require(
        value.chunk_byte_offset == offset
            && value.chunk_raw_bytes == expected
            && raw_length == usize::try_from(expected).ok(),
    )
}
