//! Encrypted append-only trajectory history with a bounded authenticated index.

pub(super) use super::history_files::archive_path;
use super::history_files::{acquire_lock, fresh_generation, new_file, reject_link};
use super::history_files::{erasure_requested, finish_erasure, request_erasure};
pub(crate) use super::trajectory_ledger::{TrajectoryStoreError, TrajectoryVideoBinding};
use super::{
    hashing::{Sha256Accumulator, sha256_hex},
    trajectory_crypto::TrajectoryCipher,
    trajectory_index::{INDEX_ENTRY_BYTES, TrajectoryIndexEntry},
    trajectory_ledger::TrajectoryLedger,
};
use crate::protocol::{
    DeliveryRoute,
    trajectory::{
        CursorTrajectoryAlgorithm, CursorTrajectoryDescriptor, CursorTrajectoryEncoding,
        CursorTrajectoryFormat, CursorTrajectoryManifestEntry, CursorTrajectoryReceipt,
        CursorTrajectoryRecord, CursorTrajectoryState, CursorTrajectoryStreamValidator,
        decode_trajectory_block,
    },
};
use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

const BLOCK_HEADER_BYTES: u64 = 36;
const BLOCK_OVERHEAD_BYTES: u64 = BLOCK_HEADER_BYTES + 16;

pub(crate) fn default_trajectory_root() -> Result<PathBuf, TrajectoryStoreError> {
    let root = std::env::var_os("LOCALAPPDATA").ok_or(TrajectoryStoreError::Io)?;
    Ok(PathBuf::from(root)
        .join("Sniptale")
        .join("NativeAgent")
        .join("temp-media")
        .join("cursor-trajectories"))
}

#[derive(Debug)]
pub(crate) struct TrajectoryArchive {
    writable: bool,
    directory: PathBuf,
    _lock: File,
    data: File,
    index: File,
    cipher: TrajectoryCipher,
    ledger: TrajectoryLedger,
}

#[derive(Debug)]
pub(crate) struct TrajectoryWriter {
    archive: TrajectoryArchive,
    stream: Sha256Accumulator,
    validator: CursorTrajectoryStreamValidator,
    generation: String,
    failed: bool,
}

pub(crate) use super::history_block_io::HistoryChunk as TrajectoryChunk;

impl TrajectoryWriter {
    pub(crate) fn descriptor(&self) -> &CursorTrajectoryDescriptor {
        self.archive.descriptor()
    }
    pub(crate) fn create(
        root: &Path,
        recording_id: &str,
        trajectory_id: &str,
        route: DeliveryRoute,
    ) -> Result<Self, TrajectoryStoreError> {
        let directory = archive_path(root, recording_id, trajectory_id)?;
        if !route.valid_local_route() {
            return Err(TrajectoryStoreError::Invalid);
        }
        fs::create_dir_all(root).map_err(|_| TrajectoryStoreError::Io)?;
        reject_link(root)?;
        // A retired identity is never recreated by a late writer.
        fs::create_dir(&directory).map_err(|_| TrajectoryStoreError::Conflict)?;
        let lock = acquire_lock(&directory, true, true)?;
        let data = new_file(&directory.join("blocks.bin"))?;
        let index = new_file(&directory.join("index.bin"))?;
        let (cipher, key) = TrajectoryCipher::create(recording_id, trajectory_id)
            .map_err(|_| TrajectoryStoreError::Authentication)?;
        let generation = fresh_generation()?;
        let ledger = TrajectoryLedger {
            version: 1,
            recording_id: recording_id.to_owned(),
            trajectory_id: trajectory_id.to_owned(),
            writer_generation: generation.clone(),
            key,
            route,
            video: None,
            video_receipt_accepted: false,
            attachment_receipt_accepted: false,
            retired: false,
            committed_data_bytes: 0,
            descriptor: CursorTrajectoryDescriptor {
                format: CursorTrajectoryFormat::V1,
                format_version: 1,
                algorithm: CursorTrajectoryAlgorithm::SedWindowV1,
                encoding: CursorTrajectoryEncoding::JsonAsciiIntV1,
                trajectory_id: trajectory_id.to_owned(),
                state: CursorTrajectoryState::Complete,
                finalized: false,
                total_blocks: 0,
                total_records: 0,
                total_samples: 0,
                total_bytes: 0,
                stream_sha256: sha256_hex(&[]),
            },
        };
        ledger.write(&directory.join("head.json"), &cipher)?;
        Ok(Self {
            archive: TrajectoryArchive {
                writable: true,
                directory,
                _lock: lock,
                data,
                index,
                cipher,
                ledger,
            },
            stream: Sha256Accumulator::new(),
            validator: CursorTrajectoryStreamValidator::default(),
            generation,
            failed: false,
        })
    }

    /// An acknowledged prefix is immutable; an exact retry is idempotent.
    pub(crate) fn append(&mut self, bytes: &[u8]) -> Result<(), TrajectoryStoreError> {
        if self.failed
            || self.archive.ledger.retired
            || self.archive.ledger.writer_generation != self.generation
        {
            return Err(TrajectoryStoreError::Retired);
        }
        let block = decode_trajectory_block(bytes).map_err(|_| TrajectoryStoreError::Invalid)?;
        let head = &self.archive.ledger;
        if block.recording_id != head.recording_id || block.trajectory_id != head.trajectory_id {
            return Err(TrajectoryStoreError::Invalid);
        }
        if block.sequence < head.descriptor.total_blocks {
            return if self.archive.read_block(block.sequence)? == bytes {
                Ok(())
            } else {
                Err(TrajectoryStoreError::Conflict)
            };
        }
        if head.descriptor.finalized {
            return Err(TrajectoryStoreError::Retired);
        }
        let mut validator = self.validator.clone();
        validator
            .accept(&block)
            .map_err(|_| TrajectoryStoreError::Invalid)?;
        let mut stream = self.stream.clone();
        let length = u32::try_from(bytes.len()).map_err(|_| TrajectoryStoreError::Invalid)?;
        stream.update(&length.to_le_bytes());
        stream.update(bytes);
        let mut next = head.clone();
        let descriptor = &mut next.descriptor;
        descriptor.total_blocks += 1;
        descriptor.total_records +=
            u64::try_from(block.records.len()).map_err(|_| TrajectoryStoreError::Invalid)?;
        let samples = u16::try_from(
            block
                .records
                .iter()
                .filter(|record| matches!(record, CursorTrajectoryRecord::Sample(_)))
                .count(),
        )
        .map_err(|_| TrajectoryStoreError::Invalid)?;
        descriptor.total_samples += u64::from(samples);
        descriptor.total_bytes += u64::from(length);
        descriptor.stream_sha256 = stream.clone().finalize_hex();
        if validator.incomplete() {
            descriptor.state = CursorTrajectoryState::Incomplete;
        }
        let entry = TrajectoryIndexEntry {
            manifest: CursorTrajectoryManifestEntry {
                sequence: block.sequence,
                start_time_us: block.start_time_us,
                end_time_us: block.end_time_us,
                record_count: u16::try_from(block.records.len())
                    .map_err(|_| TrajectoryStoreError::Invalid)?,
                sample_count: samples,
                byte_length: length,
                block_sha256: sha256_hex(bytes),
                prefix_sha256: descriptor.stream_sha256.clone(),
            },
            data_offset: head.committed_data_bytes,
            cumulative_records: descriptor.total_records,
            cumulative_samples: descriptor.total_samples,
            cumulative_bytes: descriptor.total_bytes,
        };
        next.committed_data_bytes = head
            .committed_data_bytes
            .checked_add(BLOCK_OVERHEAD_BYTES + u64::from(length))
            .ok_or(TrajectoryStoreError::Invalid)?;
        // Failure after a write may leave an orphan tail. Never continue this writer.
        if let Err(error) = self.archive.commit_block(bytes, &entry, &next) {
            self.failed = true;
            return Err(error);
        }
        self.archive.ledger = next;
        self.stream = stream;
        self.validator = validator;
        Ok(())
    }

    pub(crate) fn finalize(
        &mut self,
        incomplete: bool,
    ) -> Result<CursorTrajectoryDescriptor, TrajectoryStoreError> {
        if self.failed || self.archive.ledger.retired {
            return Err(TrajectoryStoreError::Retired);
        }
        if !self.archive.ledger.descriptor.finalized {
            let mut next = self.archive.ledger.clone();
            next.descriptor.finalized = true;
            if incomplete || !self.validator.closed_prefix() {
                next.descriptor.state = CursorTrajectoryState::Incomplete;
            }
            next.write(
                &self.archive.directory.join("head.json"),
                &self.archive.cipher,
            )?;
            self.archive.ledger = next;
        }
        Ok(self.archive.ledger.descriptor.clone())
    }

    #[cfg(test)]
    pub(crate) fn retire(&mut self) -> Result<(), TrajectoryStoreError> {
        self.failed = true;
        self.archive.retire()
    }

    pub(crate) fn into_archive(self) -> TrajectoryArchive {
        self.archive
    }
}

impl TrajectoryArchive {
    fn require_mutation(&self) -> Result<(), TrajectoryStoreError> {
        if self.writable {
            Ok(())
        } else {
            Err(TrajectoryStoreError::Invalid)
        }
    }
    /// Explicit recovery seals the verified committed prefix; read paths never repair it.
    pub(crate) fn recover_interrupted(&mut self) -> Result<(), TrajectoryStoreError> {
        self.require_mutation()?;
        if self.ledger.retired {
            return Err(TrajectoryStoreError::Retired);
        }
        if self.ledger.descriptor.finalized {
            return Ok(());
        }
        // The authenticated head and final index entry already commit the exact prefix.
        // Recovery does not append or reinterpret records. Each later block read verifies its
        // authenticated ciphertext and committed hash before any bytes leave this owner.
        self.validate_head()?;
        let mut next = self.ledger.clone();
        next.writer_generation = fresh_generation()?;
        next.descriptor.finalized = true;
        next.descriptor.state = if next.descriptor.total_blocks == 0 {
            CursorTrajectoryState::Unavailable
        } else {
            CursorTrajectoryState::Incomplete
        };
        next.write(&self.directory.join("head.json"), &self.cipher)?;
        self.ledger = next;
        Ok(())
    }

    pub(crate) fn chunk(
        &mut self,
        sequence: u64,
        index: u8,
    ) -> Result<TrajectoryChunk, TrajectoryStoreError> {
        let entry = self.read_entry(sequence)?;
        super::history_block_io::chunk(&mut self.data, &self.cipher, &entry, index)
    }

    pub(crate) fn bind_video(
        &mut self,
        video: TrajectoryVideoBinding,
    ) -> Result<(), TrajectoryStoreError> {
        self.require_mutation()?;
        if self.ledger.retired {
            return Err(TrajectoryStoreError::Retired);
        }
        if let Some(existing) = &self.ledger.video {
            return if *existing == video {
                Ok(())
            } else {
                Err(TrajectoryStoreError::Conflict)
            };
        }
        let mut next = self.ledger.clone();
        next.video = Some(video);
        next.write(&self.directory.join("head.json"), &self.cipher)?;
        self.ledger = next;
        Ok(())
    }

    /// The caller has already authorized the exact media assignment and current lease.
    pub(crate) fn accept_video_receipt(
        &mut self,
        bytes: u64,
        sha256: &str,
    ) -> Result<(), TrajectoryStoreError> {
        self.require_mutation()?;
        if self.ledger.retired {
            return Err(TrajectoryStoreError::Retired);
        }
        if !self
            .ledger
            .video
            .as_ref()
            .is_some_and(|video| video.bytes == bytes && video.sha256 == sha256)
        {
            return Err(TrajectoryStoreError::Invalid);
        }
        let mut next = self.ledger.clone();
        next.video_receipt_accepted = true;
        next.write(&self.directory.join("head.json"), &self.cipher)?;
        self.ledger = next;
        Ok(())
    }

    pub(crate) fn accept_attachment_receipt(
        &mut self,
        receipt: &CursorTrajectoryReceipt,
    ) -> Result<(), TrajectoryStoreError> {
        self.require_mutation()?;
        if self.ledger.retired {
            return Err(TrajectoryStoreError::Retired);
        }
        let descriptor = &self.ledger.descriptor;
        if !descriptor.finalized
            || receipt.version != 1
            || !receipt.durable
            || receipt.format != descriptor.format
            || receipt.state != descriptor.state
            || receipt.total_blocks != descriptor.total_blocks
            || receipt.total_records != descriptor.total_records
            || receipt.total_samples != descriptor.total_samples
            || receipt.total_bytes != descriptor.total_bytes
            || receipt.stream_sha256 != descriptor.stream_sha256
        {
            return Err(TrajectoryStoreError::Invalid);
        }
        let mut next = self.ledger.clone();
        next.attachment_receipt_accepted = true;
        next.write(&self.directory.join("head.json"), &self.cipher)?;
        self.ledger = next;
        Ok(())
    }

    pub(crate) fn both_receipts_accepted(&self) -> bool {
        self.ledger.video_receipt_accepted && self.ledger.attachment_receipt_accepted
    }

    fn retire(&mut self) -> Result<(), TrajectoryStoreError> {
        self.require_mutation()?;
        let mut next = self.ledger.clone();
        next.retired = true;
        next.writer_generation = fresh_generation()?;
        next.write(&self.directory.join("head.json"), &self.cipher)?;
        self.ledger = next;
        Ok(())
    }

    /// Explicit privacy erase retains the directory identity as a tombstone.
    pub(crate) fn erase(mut self) -> Result<(), TrajectoryStoreError> {
        self.require_mutation()?;
        request_erasure(&self.directory)?;
        self.retire()?;
        let directory = self.directory.clone();
        // Join and retire the producer before calling this consuming operation.
        drop(self);
        finish_erasure(&directory)
    }

    pub(crate) fn open(
        root: &Path,
        recording_id: &str,
        trajectory_id: &str,
    ) -> Result<Self, TrajectoryStoreError> {
        Self::open_access(root, recording_id, trajectory_id, true)
    }

    pub(crate) fn read(
        root: &Path,
        recording_id: &str,
        trajectory_id: &str,
    ) -> Result<Self, TrajectoryStoreError> {
        Self::open_access(root, recording_id, trajectory_id, false)
    }

    fn open_access(
        root: &Path,
        recording_id: &str,
        trajectory_id: &str,
        writable: bool,
    ) -> Result<Self, TrajectoryStoreError> {
        let directory = archive_path(root, recording_id, trajectory_id)?;
        reject_link(root)?;
        reject_link(&directory)?;
        let lock = acquire_lock(&directory, false, writable)?;
        if erasure_requested(&directory)? {
            return Err(TrajectoryStoreError::Retired);
        }
        for name in ["head.json", "blocks.bin", "index.bin"] {
            reject_link(&directory.join(name))?;
        }
        let (ledger, cipher) = TrajectoryLedger::read(&directory.join("head.json"))?;
        if ledger.recording_id != recording_id || ledger.trajectory_id != trajectory_id {
            return Err(TrajectoryStoreError::Invalid);
        }
        if ledger.retired {
            return Err(TrajectoryStoreError::Retired);
        }
        let data =
            File::open(directory.join("blocks.bin")).map_err(|_| TrajectoryStoreError::Io)?;
        let index =
            File::open(directory.join("index.bin")).map_err(|_| TrajectoryStoreError::Io)?;
        let mut archive = Self {
            writable,
            directory,
            _lock: lock,
            data,
            index,
            cipher,
            ledger,
        };
        archive.validate_head()?;
        Ok(archive)
    }

    pub(crate) fn descriptor(&self) -> &CursorTrajectoryDescriptor {
        &self.ledger.descriptor
    }

    pub(crate) fn block_entry(
        &mut self,
        sequence: u64,
    ) -> Result<CursorTrajectoryManifestEntry, TrajectoryStoreError> {
        self.read_entry(sequence).map(|entry| entry.manifest)
    }

    #[cfg(test)]
    pub(crate) fn release_after_receipts(self) -> Result<(), TrajectoryStoreError> {
        self.require_mutation()?;
        if !self.both_receipts_accepted() {
            return Err(TrajectoryStoreError::Invalid);
        }
        let directory = self.directory.clone();
        drop(self);
        for name in ["blocks.bin", "index.bin"] {
            match fs::remove_file(directory.join(name)) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => return Err(TrajectoryStoreError::Io),
            }
        }
        Ok(())
    }

    pub(crate) fn manifest(
        &mut self,
        from: u64,
        prefix_sha256: &str,
    ) -> Result<Vec<CursorTrajectoryManifestEntry>, TrajectoryStoreError> {
        if from > self.ledger.descriptor.total_blocks {
            return Err(TrajectoryStoreError::Invalid);
        }
        let expected = if from == 0 {
            sha256_hex(&[])
        } else {
            self.read_entry(from - 1)?.manifest.prefix_sha256
        };
        if prefix_sha256 != expected {
            return Err(TrajectoryStoreError::Conflict);
        }
        let end = from
            .saturating_add(32)
            .min(self.ledger.descriptor.total_blocks);
        (from..end)
            .map(|sequence| self.read_entry(sequence).map(|entry| entry.manifest))
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn block_at_time(
        &mut self,
        time_us: u64,
    ) -> Result<Option<CursorTrajectoryManifestEntry>, TrajectoryStoreError> {
        let mut lower = 0;
        let mut upper = self.ledger.descriptor.total_blocks;
        while lower < upper {
            let middle = lower + (upper - lower) / 2;
            let entry = self.read_entry(middle)?;
            if entry.manifest.end_time_us < time_us {
                lower = middle + 1;
            } else {
                upper = middle;
            }
        }
        if lower == self.ledger.descriptor.total_blocks {
            Ok(None)
        } else {
            self.read_entry(lower).map(|entry| Some(entry.manifest))
        }
    }

    pub(crate) fn read_block(&mut self, sequence: u64) -> Result<Vec<u8>, TrajectoryStoreError> {
        let entry = self.read_entry(sequence)?;
        super::history_block_io::read_block(&mut self.data, &self.cipher, &entry)
    }

    fn read_entry(&mut self, sequence: u64) -> Result<TrajectoryIndexEntry, TrajectoryStoreError> {
        if self.ledger.retired {
            return Err(TrajectoryStoreError::Retired);
        }
        super::history_block_io::read_entry(
            &mut self.index,
            &self.cipher,
            sequence,
            self.ledger.descriptor.total_blocks,
            self.ledger.committed_data_bytes,
        )
    }

    fn commit_block(
        &mut self,
        bytes: &[u8],
        entry: &TrajectoryIndexEntry,
        next: &TrajectoryLedger,
    ) -> Result<(), TrajectoryStoreError> {
        super::history_block_io::commit_block(
            &mut self.data,
            &mut self.index,
            &self.cipher,
            bytes,
            entry,
        )?;
        next.write(&self.directory.join("head.json"), &self.cipher)
    }

    fn validate_head(&mut self) -> Result<(), TrajectoryStoreError> {
        let descriptor = self.ledger.descriptor.clone();
        if self
            .data
            .metadata()
            .map_err(|_| TrajectoryStoreError::Io)?
            .len()
            < self.ledger.committed_data_bytes
            || self
                .index
                .metadata()
                .map_err(|_| TrajectoryStoreError::Io)?
                .len()
                < descriptor
                    .total_blocks
                    .checked_mul(INDEX_ENTRY_BYTES as u64)
                    .ok_or(TrajectoryStoreError::Invalid)?
        {
            return Err(TrajectoryStoreError::Invalid);
        }
        if descriptor.total_blocks == 0 {
            if descriptor.total_bytes != 0
                || descriptor.total_records != 0
                || descriptor.total_samples != 0
                || self.ledger.committed_data_bytes != 0
                || descriptor.stream_sha256 != sha256_hex(&[])
            {
                return Err(TrajectoryStoreError::Invalid);
            }
        } else {
            let last = self.read_entry(descriptor.total_blocks - 1)?;
            if last.cumulative_bytes != descriptor.total_bytes
                || last.cumulative_records != descriptor.total_records
                || last.cumulative_samples != descriptor.total_samples
                || last.manifest.prefix_sha256 != descriptor.stream_sha256
                || last.data_offset + BLOCK_OVERHEAD_BYTES + u64::from(last.manifest.byte_length)
                    != self.ledger.committed_data_bytes
            {
                return Err(TrajectoryStoreError::Invalid);
            }
        }
        Ok(())
    }
}
