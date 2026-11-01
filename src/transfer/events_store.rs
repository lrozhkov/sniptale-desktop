//! Protected recording-event blocks. Source bytes are never reserialized for storage or identity.

use super::{
    events_ledger::EventsLedger,
    hashing::{Sha256Accumulator, sha256_hex},
    history_files::{acquire_lock, archive_path, fresh_generation, new_file, reject_link},
    trajectory_crypto::{HistoryProtection, TrajectoryCipher},
    trajectory_index::{INDEX_ENTRY_BYTES, TrajectoryIndexEntry},
    trajectory_ledger::{TrajectoryStoreError, TrajectoryVideoBinding},
};
use crate::protocol::{
    DeliveryRoute,
    recording_events::{
        EventAlgorithm, EventDescriptor, EventEncoding, EventFormat, EventManifestEntry,
        EventReceipt, EventStreamValidator, decode_event_block,
    },
    trajectory::{CursorTrajectoryManifestEntry, CursorTrajectoryState},
};

pub(crate) fn default_events_root() -> Result<PathBuf, TrajectoryStoreError> {
    let root = std::env::var_os("LOCALAPPDATA").ok_or(TrajectoryStoreError::Io)?;
    Ok(PathBuf::from(root)
        .join("Sniptale")
        .join("NativeAgent")
        .join("temp-media")
        .join("recording-events"))
}
use super::history_files::{erasure_requested, finish_erasure, request_erasure};
use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

#[derive(Debug)]
pub(crate) struct EventsArchive {
    directory: PathBuf,
    writable: bool,
    _lock: File,
    data: File,
    index: File,
    cipher: TrajectoryCipher,
    ledger: EventsLedger,
}

#[derive(Debug)]
pub(crate) struct EventsWriter {
    archive: EventsArchive,
    stream: Sha256Accumulator,
    validator: EventStreamValidator,
    generation: String,
    failed: bool,
}

impl EventsWriter {
    pub(crate) fn create(
        root: &Path,
        recording: &str,
        source: &str,
        route: DeliveryRoute,
    ) -> Result<Self, TrajectoryStoreError> {
        let directory = archive_path(root, recording, source)?;
        if !route.valid_local_route() {
            return Err(TrajectoryStoreError::Invalid);
        }
        fs::create_dir_all(root).map_err(|_| TrajectoryStoreError::Io)?;
        reject_link(root)?;
        fs::create_dir(&directory).map_err(|_| TrajectoryStoreError::Conflict)?;
        let lock = acquire_lock(&directory, true, true)?;
        let data = new_file(&directory.join("blocks.bin"))?;
        let index = new_file(&directory.join("index.bin"))?;
        let (cipher, key) =
            TrajectoryCipher::create_for(recording, source, HistoryProtection::Events)
                .map_err(|_| TrajectoryStoreError::Authentication)?;
        let generation = fresh_generation()?;
        let ledger = EventsLedger {
            version: 1,
            recording_id: recording.to_owned(),
            event_stream_id: source.to_owned(),
            writer_generation: generation.clone(),
            key,
            route,
            video: None,
            video_receipt_accepted: false,
            event_receipt_accepted: false,
            retired: false,
            committed_data_bytes: 0,
            descriptor: EventDescriptor {
                format: EventFormat::V1,
                format_version: 1,
                algorithm: EventAlgorithm::LosslessV1,
                encoding: EventEncoding::JsonUtf8V1,
                event_stream_id: source.to_owned(),
                state: CursorTrajectoryState::Complete,
                finalized: false,
                total_blocks: 0,
                total_records: 0,
                total_bytes: 0,
                stream_sha256: sha256_hex(&[]),
            },
        };
        ledger.write(&directory.join("head.json"), &cipher)?;
        Ok(Self {
            archive: EventsArchive {
                directory,
                writable: true,
                _lock: lock,
                data,
                index,
                cipher,
                ledger,
            },
            stream: Sha256Accumulator::new(),
            validator: EventStreamValidator::default(),
            generation,
            failed: false,
        })
    }

    pub(crate) fn descriptor(&self) -> &EventDescriptor {
        self.archive.descriptor()
    }

    pub(crate) fn append(&mut self, bytes: &[u8]) -> Result<(), TrajectoryStoreError> {
        if self.failed
            || self.archive.ledger.retired
            || self.generation != self.archive.ledger.writer_generation
        {
            return Err(TrajectoryStoreError::Retired);
        }
        let block = decode_event_block(bytes).map_err(|_| TrajectoryStoreError::Invalid)?;
        if block.recording_id != self.archive.ledger.recording_id
            || block.event_stream_id != self.archive.ledger.event_stream_id
        {
            return Err(TrajectoryStoreError::Invalid);
        }
        if block.sequence < self.descriptor().total_blocks {
            return if self.archive.read_block(block.sequence)? == bytes {
                Ok(())
            } else {
                Err(TrajectoryStoreError::Conflict)
            };
        }
        if self.descriptor().finalized {
            return Err(TrajectoryStoreError::Retired);
        }
        let mut validator = self.validator.clone();
        validator
            .accept(&block)
            .map_err(|_| TrajectoryStoreError::Invalid)?;
        let length = u32::try_from(bytes.len()).map_err(|_| TrajectoryStoreError::Invalid)?;
        let count =
            u16::try_from(block.records.len()).map_err(|_| TrajectoryStoreError::Invalid)?;
        let mut stream = self.stream.clone();
        stream.update(&length.to_le_bytes());
        stream.update(bytes);
        let mut next = self.archive.ledger.clone();
        let descriptor = &mut next.descriptor;
        descriptor.total_blocks = descriptor
            .total_blocks
            .checked_add(1)
            .ok_or(TrajectoryStoreError::Invalid)?;
        descriptor.total_records = descriptor
            .total_records
            .checked_add(u64::from(count))
            .ok_or(TrajectoryStoreError::Invalid)?;
        descriptor.total_bytes = descriptor
            .total_bytes
            .checked_add(u64::from(length))
            .ok_or(TrajectoryStoreError::Invalid)?;
        descriptor.stream_sha256 = stream.clone().finalize_hex();
        if validator.incomplete() {
            descriptor.state = CursorTrajectoryState::Incomplete;
        }
        descriptor
            .validate()
            .map_err(|_| TrajectoryStoreError::Invalid)?;
        let entry = TrajectoryIndexEntry {
            manifest: CursorTrajectoryManifestEntry {
                sequence: block.sequence,
                start_time_us: block.start_time_us,
                end_time_us: block.end_time_us,
                record_count: count,
                sample_count: 0,
                byte_length: length,
                block_sha256: sha256_hex(bytes),
                prefix_sha256: descriptor.stream_sha256.clone(),
            },
            data_offset: self.archive.ledger.committed_data_bytes,
            cumulative_records: descriptor.total_records,
            cumulative_samples: 0,
            cumulative_bytes: descriptor.total_bytes,
        };
        next.committed_data_bytes = entry
            .data_offset
            .checked_add(52 + u64::from(length))
            .ok_or(TrajectoryStoreError::Invalid)?;
        let result = super::history_block_io::commit_block(
            &mut self.archive.data,
            &mut self.archive.index,
            &self.archive.cipher,
            bytes,
            &entry,
        )
        .and_then(|()| {
            next.write(
                &self.archive.directory.join("head.json"),
                &self.archive.cipher,
            )
        });
        if let Err(error) = result {
            self.failed = true;
            return Err(error);
        }
        self.archive.ledger = next;
        self.validator = validator;
        self.stream = stream;
        Ok(())
    }

    pub(crate) fn finalize(
        &mut self,
        incomplete: bool,
    ) -> Result<EventDescriptor, TrajectoryStoreError> {
        if self.failed || self.archive.ledger.retired {
            return Err(TrajectoryStoreError::Retired);
        }
        if !self.descriptor().finalized {
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
        Ok(self.descriptor().clone())
    }

    pub(crate) fn into_archive(self) -> EventsArchive {
        self.archive
    }
}

impl EventsArchive {
    pub(crate) fn descriptor(&self) -> &EventDescriptor {
        &self.ledger.descriptor
    }

    pub(crate) fn open(
        root: &Path,
        recording: &str,
        source: &str,
    ) -> Result<Self, TrajectoryStoreError> {
        Self::access(root, recording, source, true)
    }

    pub(crate) fn read(
        root: &Path,
        recording: &str,
        source: &str,
    ) -> Result<Self, TrajectoryStoreError> {
        Self::access(root, recording, source, false)
    }

    fn access(
        root: &Path,
        recording: &str,
        source: &str,
        writable: bool,
    ) -> Result<Self, TrajectoryStoreError> {
        let directory = archive_path(root, recording, source)?;
        reject_link(root)?;
        reject_link(&directory)?;
        let lock = acquire_lock(&directory, false, writable)?;
        if erasure_requested(&directory)? {
            return Err(TrajectoryStoreError::Retired);
        }
        for name in ["head.json", "blocks.bin", "index.bin"] {
            reject_link(&directory.join(name))?;
        }
        let (ledger, cipher) = EventsLedger::read(&directory.join("head.json"))?;
        if ledger.recording_id != recording || ledger.event_stream_id != source {
            return Err(TrajectoryStoreError::Invalid);
        }
        if ledger.retired {
            return Err(TrajectoryStoreError::Retired);
        }
        let data = OpenOptions::new()
            .read(true)
            .write(writable)
            .open(directory.join("blocks.bin"))
            .map_err(|_| TrajectoryStoreError::Io)?;
        let index = OpenOptions::new()
            .read(true)
            .write(writable)
            .open(directory.join("index.bin"))
            .map_err(|_| TrajectoryStoreError::Io)?;
        let mut archive = Self {
            directory,
            writable,
            _lock: lock,
            data,
            index,
            cipher,
            ledger,
        };
        archive.validate_head()?;
        Ok(archive)
    }

    fn mutation(&self) -> Result<(), TrajectoryStoreError> {
        if !self.writable {
            return Err(TrajectoryStoreError::Invalid);
        }
        if self.ledger.retired {
            return Err(TrajectoryStoreError::Retired);
        }
        Ok(())
    }

    pub(crate) fn bind_video(
        &mut self,
        video: TrajectoryVideoBinding,
    ) -> Result<(), TrajectoryStoreError> {
        self.mutation()?;
        if let Some(existing) = &self.ledger.video {
            return if *existing == video {
                Ok(())
            } else {
                Err(TrajectoryStoreError::Conflict)
            };
        }
        let mut next = self.ledger.clone();
        next.video = Some(video);
        self.commit_head(next)
    }

    pub(crate) fn accept_video_receipt(
        &mut self,
        bytes: u64,
        sha: &str,
    ) -> Result<(), TrajectoryStoreError> {
        self.mutation()?;
        if !self
            .ledger
            .video
            .as_ref()
            .is_some_and(|video| video.bytes == bytes && video.sha256 == sha)
        {
            return Err(TrajectoryStoreError::Invalid);
        }
        let mut next = self.ledger.clone();
        next.video_receipt_accepted = true;
        self.commit_head(next)
    }

    pub(crate) fn accept_event_receipt(
        &mut self,
        receipt: &EventReceipt,
    ) -> Result<(), TrajectoryStoreError> {
        self.mutation()?;
        if !receipt.matches(self.descriptor()) {
            return Err(TrajectoryStoreError::Invalid);
        }
        let mut next = self.ledger.clone();
        next.event_receipt_accepted = true;
        self.commit_head(next)
    }

    pub(crate) fn both_receipts_accepted(&self) -> bool {
        self.ledger.video_receipt_accepted && self.ledger.event_receipt_accepted
    }

    pub(crate) fn recover_interrupted(&mut self) -> Result<(), TrajectoryStoreError> {
        self.mutation()?;
        if self.descriptor().finalized {
            return Ok(());
        }
        let mut next = self.ledger.clone();
        next.writer_generation = fresh_generation()?;
        next.descriptor.finalized = true;
        next.descriptor.state = if next.descriptor.total_blocks == 0 {
            CursorTrajectoryState::Unavailable
        } else {
            CursorTrajectoryState::Incomplete
        };
        self.commit_head(next)
    }

    fn commit_head(&mut self, next: EventsLedger) -> Result<(), TrajectoryStoreError> {
        next.write(&self.directory.join("head.json"), &self.cipher)?;
        self.ledger = next;
        Ok(())
    }

    fn entry(&mut self, sequence: u64) -> Result<TrajectoryIndexEntry, TrajectoryStoreError> {
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

    pub(crate) fn read_block(&mut self, sequence: u64) -> Result<Vec<u8>, TrajectoryStoreError> {
        let entry = self.entry(sequence)?;
        super::history_block_io::read_block(&mut self.data, &self.cipher, &entry)
    }

    pub(crate) fn block_entry(
        &mut self,
        sequence: u64,
    ) -> Result<EventManifestEntry, TrajectoryStoreError> {
        let item = self.entry(sequence)?.manifest;
        Ok(EventManifestEntry {
            sequence: item.sequence,
            start_time_us: item.start_time_us,
            end_time_us: item.end_time_us,
            record_count: item.record_count,
            byte_length: item.byte_length,
            block_sha256: item.block_sha256,
            prefix_sha256: item.prefix_sha256,
        })
    }

    pub(crate) fn chunk(
        &mut self,
        sequence: u64,
        index: u8,
    ) -> Result<super::trajectory_store::TrajectoryChunk, TrajectoryStoreError> {
        let entry = self.entry(sequence)?;
        super::history_block_io::chunk(&mut self.data, &self.cipher, &entry, index)
    }

    pub(crate) fn manifest(
        &mut self,
        from: u64,
        prefix: &str,
    ) -> Result<Vec<EventManifestEntry>, TrajectoryStoreError> {
        if from > self.descriptor().total_blocks {
            return Err(TrajectoryStoreError::Invalid);
        }
        let expected = if from == 0 {
            sha256_hex(&[])
        } else {
            self.entry(from - 1)?.manifest.prefix_sha256
        };
        if prefix != expected {
            return Err(TrajectoryStoreError::Conflict);
        }
        let end = from.saturating_add(32).min(self.descriptor().total_blocks);
        (from..end)
            .map(|sequence| {
                self.entry(sequence).map(|entry| {
                    let item = entry.manifest;
                    EventManifestEntry {
                        sequence: item.sequence,
                        start_time_us: item.start_time_us,
                        end_time_us: item.end_time_us,
                        record_count: item.record_count,
                        byte_length: item.byte_length,
                        block_sha256: item.block_sha256,
                        prefix_sha256: item.prefix_sha256,
                    }
                })
            })
            .collect()
    }

    pub(crate) fn erase(mut self) -> Result<(), TrajectoryStoreError> {
        self.mutation()?;
        request_erasure(&self.directory)?;
        let mut next = self.ledger.clone();
        next.retired = true;
        next.writer_generation = fresh_generation()?;
        self.commit_head(next)?;
        let directory = self.directory.clone();
        drop(self);
        finish_erasure(&directory)
    }

    fn validate_head(&mut self) -> Result<(), TrajectoryStoreError> {
        let descriptor = self.descriptor().clone();
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
            if self.ledger.committed_data_bytes != 0 {
                return Err(TrajectoryStoreError::Invalid);
            }
        } else {
            let last = self.entry(descriptor.total_blocks - 1)?;
            if last.cumulative_records != descriptor.total_records
                || last.cumulative_bytes != descriptor.total_bytes
                || last.cumulative_samples != 0
                || last.manifest.prefix_sha256 != descriptor.stream_sha256
                || last
                    .data_offset
                    .checked_add(52 + u64::from(last.manifest.byte_length))
                    != Some(self.ledger.committed_data_bytes)
            {
                return Err(TrajectoryStoreError::Invalid);
            }
        }
        Ok(())
    }
}
