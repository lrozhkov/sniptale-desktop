//! Attachment envelopes have their own limits and never inherit video chunk quotas.

use super::{
    AppRecordingAttachmentChunkMessage, AppRecordingAttachmentManifestMessage,
    CursorTrajectoryAlgorithm, CursorTrajectoryDescriptor, CursorTrajectoryEncoding,
    CursorTrajectoryReceipt, MAX_SAFE_INTEGER, NativeTrajectoryAttachmentCapabilities,
    TrajectoryContractError, valid_trajectory_hash, valid_trajectory_id,
    validate_trajectory_descriptor,
};
use crate::protocol::WireMessage;

pub(crate) fn validate_attachment_message(
    message: &WireMessage,
) -> Result<(), TrajectoryContractError> {
    match message {
        WireMessage::ExtensionRecordingAttachmentManifestRequest(value) => {
            identity(
                &value.recording_id,
                &value.trajectory_id,
                Some(&value.request_id),
            )?;
            require(
                value.next_sequence <= MAX_SAFE_INTEGER
                    && valid_trajectory_hash(&value.prefix_sha256),
            )
        }
        WireMessage::ExtensionRecordingAttachmentChunkRequest(value) => {
            identity(
                &value.recording_id,
                &value.trajectory_id,
                Some(&value.request_id),
            )?;
            require(value.sequence <= MAX_SAFE_INTEGER && value.chunk_index < 2)
        }
        WireMessage::ExtensionRecordingAttachmentAck(value) => {
            identity(
                &value.recording_id,
                &value.trajectory_id,
                Some(&value.request_id),
            )?;
            receipt(&value.receipt, &value.trajectory_id)
        }
        WireMessage::AppRecordingAttachmentManifest(value) => manifest(value),
        WireMessage::AppRecordingAttachmentChunk(value) => chunk(value),
        WireMessage::AppRecordingAttachmentAvailable(value) => {
            identity(&value.recording_id, &value.trajectory_id, None)?;
            validate_trajectory_descriptor(&value.descriptor)?;
            require(
                value.trajectory_id == value.descriptor.trajectory_id
                    && valid_trajectory_hash(&value.video_sha256),
            )
        }
        WireMessage::AppRecordingStopped(value) => {
            if let Some(descriptor) = &value.cursor_trajectory {
                validate_trajectory_descriptor(descriptor)?;
            }
            Ok(())
        }
        WireMessage::AppHello(value) => {
            if let Some(attachments) = &value.capabilities.attachments {
                require(*attachments == NativeTrajectoryAttachmentCapabilities::v1())?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

fn identity(
    recording: &str,
    trajectory: &str,
    request: Option<&str>,
) -> Result<(), TrajectoryContractError> {
    require(
        super::validation::valid_id(recording)
            && valid_trajectory_id(trajectory)
            && request.is_none_or(super::validation::valid_id),
    )
}

fn require(condition: bool) -> Result<(), TrajectoryContractError> {
    if condition {
        Ok(())
    } else {
        Err(TrajectoryContractError::Format)
    }
}

fn receipt(
    value: &CursorTrajectoryReceipt,
    trajectory: &str,
) -> Result<(), TrajectoryContractError> {
    require(value.version == 1 && value.durable)?;
    validate_trajectory_descriptor(&CursorTrajectoryDescriptor {
        format: value.format,
        format_version: 1,
        algorithm: CursorTrajectoryAlgorithm::SedWindowV1,
        encoding: CursorTrajectoryEncoding::JsonAsciiIntV1,
        trajectory_id: trajectory.to_owned(),
        state: value.state,
        finalized: true,
        total_blocks: value.total_blocks,
        total_records: value.total_records,
        total_samples: value.total_samples,
        total_bytes: value.total_bytes,
        stream_sha256: value.stream_sha256.clone(),
    })
}

fn manifest(value: &AppRecordingAttachmentManifestMessage) -> Result<(), TrajectoryContractError> {
    identity(
        &value.recording_id,
        &value.trajectory_id,
        Some(&value.request_id),
    )?;
    validate_trajectory_descriptor(&value.descriptor)?;
    require(
        value.trajectory_id == value.descriptor.trajectory_id
            && value.entries.len() <= 32
            && value.from_sequence <= value.descriptor.total_blocks,
    )?;
    let end = value
        .from_sequence
        .checked_add(
            u64::try_from(value.entries.len()).map_err(|_| TrajectoryContractError::Count)?,
        )
        .ok_or(TrajectoryContractError::Count)?;
    require(
        end <= value.descriptor.total_blocks
            && value.next_sequence == (end < value.descriptor.total_blocks).then_some(end)
            && (end == value.descriptor.total_blocks || !value.entries.is_empty()),
    )?;
    let mut previous_end = None;
    for (index, entry) in value.entries.iter().enumerate() {
        require(
            entry.sequence
                == value.from_sequence
                    + u64::try_from(index).map_err(|_| TrajectoryContractError::Count)?
                && entry.start_time_us <= entry.end_time_us
                && entry.end_time_us <= MAX_SAFE_INTEGER
                && previous_end.is_none_or(|end| entry.start_time_us >= end)
                && (1..=512).contains(&entry.record_count)
                && entry.sample_count <= entry.record_count
                && (1..=65536).contains(&entry.byte_length)
                && valid_trajectory_hash(&entry.block_sha256)
                && valid_trajectory_hash(&entry.prefix_sha256),
        )?;
        previous_end = Some(entry.end_time_us);
    }
    if end == value.descriptor.total_blocks
        && let Some(last) = value.entries.last()
    {
        require(last.prefix_sha256 == value.descriptor.stream_sha256)?;
    }
    Ok(())
}

fn chunk(value: &AppRecordingAttachmentChunkMessage) -> Result<(), TrajectoryContractError> {
    identity(
        &value.recording_id,
        &value.trajectory_id,
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
