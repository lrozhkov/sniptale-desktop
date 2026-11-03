//! Fixed authenticated entries allow bounded random access without a history array.

use super::trajectory_crypto::{TrajectoryAuthentication, TrajectoryCipher, TrajectoryCryptoError};
use crate::protocol::trajectory::{CursorTrajectoryManifestEntry, MAX_SAFE_INTEGER};

const ENTRY_PAYLOAD_BYTES: usize = 144;
pub(super) const INDEX_ENTRY_BYTES: usize = ENTRY_PAYLOAD_BYTES + 32;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TrajectoryIndexEntry {
    pub(super) manifest: CursorTrajectoryManifestEntry,
    pub(super) data_offset: u64,
    pub(super) cumulative_records: u64,
    pub(super) cumulative_samples: u64,
    pub(super) cumulative_bytes: u64,
}

impl TrajectoryIndexEntry {
    pub(super) fn encode(
        &self,
        cipher: &TrajectoryCipher,
    ) -> Result<[u8; INDEX_ENTRY_BYTES], TrajectoryCryptoError> {
        self.validate()?;
        let mut bytes = [0; INDEX_ENTRY_BYTES];
        let values = [
            self.manifest.sequence,
            self.data_offset,
            u64::from(self.manifest.byte_length),
            u64::from(self.manifest.record_count),
            u64::from(self.manifest.sample_count),
            self.manifest.start_time_us,
            self.manifest.end_time_us,
            self.cumulative_records,
            self.cumulative_samples,
            self.cumulative_bytes,
        ];
        for (slot, value) in bytes[..80].as_chunks_mut::<8>().0.iter_mut().zip(values) {
            slot.copy_from_slice(&value.to_le_bytes());
        }
        bytes[80..112].copy_from_slice(&decode_hash(&self.manifest.block_sha256)?);
        bytes[112..144].copy_from_slice(&decode_hash(&self.manifest.prefix_sha256)?);
        let tag = cipher.authenticate(
            TrajectoryAuthentication::Index,
            &bytes[..ENTRY_PAYLOAD_BYTES],
        )?;
        bytes[ENTRY_PAYLOAD_BYTES..].copy_from_slice(&tag);
        Ok(bytes)
    }

    pub(super) fn decode(
        bytes: &[u8; INDEX_ENTRY_BYTES],
        cipher: &TrajectoryCipher,
    ) -> Result<Self, TrajectoryCryptoError> {
        cipher.verify(
            TrajectoryAuthentication::Index,
            &bytes[..ENTRY_PAYLOAD_BYTES],
            &bytes[ENTRY_PAYLOAD_BYTES..],
        )?;
        let mut values = [0; 10];
        for (value, chunk) in values.iter_mut().zip(bytes[..80].as_chunks::<8>().0) {
            *value = u64::from_le_bytes(*chunk);
        }
        let entry = Self {
            manifest: CursorTrajectoryManifestEntry {
                sequence: values[0],
                byte_length: u32::try_from(values[2])
                    .map_err(|_| TrajectoryCryptoError::InvalidBlock)?,
                record_count: u16::try_from(values[3])
                    .map_err(|_| TrajectoryCryptoError::InvalidBlock)?,
                sample_count: u16::try_from(values[4])
                    .map_err(|_| TrajectoryCryptoError::InvalidBlock)?,
                start_time_us: values[5],
                end_time_us: values[6],
                block_sha256: encode_hash(&bytes[80..112]),
                prefix_sha256: encode_hash(&bytes[112..144]),
            },
            data_offset: values[1],
            cumulative_records: values[7],
            cumulative_samples: values[8],
            cumulative_bytes: values[9],
        };
        entry.validate()?;
        Ok(entry)
    }

    fn validate(&self) -> Result<(), TrajectoryCryptoError> {
        let entry = &self.manifest;
        if entry.byte_length == 0
            || entry.byte_length > 65536
            || entry.record_count == 0
            || entry.record_count > 512
            || entry.sample_count > entry.record_count
            || entry.start_time_us > entry.end_time_us
            || [
                entry.sequence,
                entry.start_time_us,
                entry.end_time_us,
                self.cumulative_records,
                self.cumulative_samples,
                self.cumulative_bytes,
            ]
            .into_iter()
            .any(|value| value > MAX_SAFE_INTEGER)
            || self.cumulative_samples > self.cumulative_records
            || self.cumulative_records < u64::from(entry.record_count)
            || self.cumulative_samples < u64::from(entry.sample_count)
            || self.cumulative_bytes < u64::from(entry.byte_length)
        {
            return Err(TrajectoryCryptoError::InvalidBlock);
        }
        Ok(())
    }
}

fn decode_hash(value: &str) -> Result<[u8; 32], TrajectoryCryptoError> {
    if !crate::protocol::trajectory::valid_trajectory_hash(value) {
        return Err(TrajectoryCryptoError::InvalidBlock);
    }
    let mut decoded = [0; 32];
    for (output, pair) in decoded.iter_mut().zip(value.as_bytes().as_chunks::<2>().0) {
        let digit = |byte: u8| match byte {
            b'0'..=b'9' => byte - b'0',
            _ => byte - b'a' + 10,
        };
        *output = digit(pair[0]) * 16 + digit(pair[1]);
    }
    Ok(decoded)
}

fn encode_hash(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(b"0123456789abcdef"[usize::from(byte >> 4)]));
        encoded.push(char::from(b"0123456789abcdef"[usize::from(byte & 15)]));
    }
    encoded
}
