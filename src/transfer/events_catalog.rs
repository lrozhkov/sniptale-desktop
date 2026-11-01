//! Private attachment discovery survives release of the independently acknowledged video.

use super::history_files::{erasure_requested, finish_erasure};
use super::{
    browser_delivery::{BrowserDeliveryStore, BrowserMediaKind},
    events_ledger::EventsLedger,
    events_store::EventsArchive,
    history_files::archive_path,
    trajectory_ledger::{TrajectoryStoreError, TrajectoryVideoBinding, check_private_path},
};
use crate::protocol::{DeliveryRoute, recording_events::EventDescriptor};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub(crate) struct PendingEvents {
    pub(crate) recording_id: String,
    pub(crate) descriptor: EventDescriptor,
    pub(crate) video: Option<TrajectoryVideoBinding>,
    pub(crate) video_receipt_accepted: bool,
    pub(crate) event_receipt_accepted: bool,
    pub(crate) delivery_route: DeliveryRoute,
}

#[derive(Clone, Debug)]
pub(crate) struct EventsCatalog {
    root: PathBuf,
}

impl EventsCatalog {
    pub(crate) fn erase_acknowledged(
        &self,
        recording: &str,
        source: &str,
    ) -> Result<(), TrajectoryStoreError> {
        let archive = EventsArchive::open(&self.root, recording, source)?;
        if !archive.both_receipts_accepted() {
            return Err(TrajectoryStoreError::Invalid);
        }
        archive.erase()
    }
    pub(crate) const fn open(root: PathBuf) -> Self {
        Self { root }
    }

    pub(crate) fn lookup(
        &self,
        recording: &str,
        source: &str,
    ) -> Result<Option<PendingEvents>, TrajectoryStoreError> {
        let directory = archive_path(&self.root, recording, source)?;
        if !exists(&directory)? {
            return Ok(None);
        }
        check_private_path(&self.root, false)?;
        check_private_path(&directory, false)?;
        let Some(ledger) = read_head(&directory)? else {
            return Ok(None);
        };
        if ledger.recording_id != recording || ledger.event_stream_id != source {
            return Err(TrajectoryStoreError::Invalid);
        }
        Ok((!ledger.retired).then(|| pending(ledger)))
    }

    pub(crate) fn authorized(
        &self,
        recording: &str,
        source: &str,
        target: &str,
        assignments: &BrowserDeliveryStore,
    ) -> Result<PendingEvents, TrajectoryStoreError> {
        if assignments
            .target(recording, BrowserMediaKind::Recording)
            .map_err(|_| TrajectoryStoreError::Invalid)?
            .as_deref()
            != Some(target)
        {
            return Err(TrajectoryStoreError::Invalid);
        }
        let item = self
            .lookup(recording, source)?
            .ok_or(TrajectoryStoreError::Invalid)?;
        assignments
            .authorize_route(
                recording,
                BrowserMediaKind::Recording,
                &item.delivery_route,
                target,
            )
            .map_err(|_| TrajectoryStoreError::Invalid)?;
        Ok(item)
    }

    pub(crate) fn authorized_read(
        &self,
        recording: &str,
        source: &str,
        target: &str,
        assignments: &BrowserDeliveryStore,
    ) -> Result<EventsArchive, TrajectoryStoreError> {
        self.authorized(recording, source, target, assignments)?;
        EventsArchive::read(&self.root, recording, source)
    }

    pub(crate) fn authorized_mutation(
        &self,
        recording: &str,
        source: &str,
        target: &str,
        assignments: &BrowserDeliveryStore,
    ) -> Result<EventsArchive, TrajectoryStoreError> {
        self.authorized(recording, source, target, assignments)?;
        EventsArchive::open(&self.root, recording, source)
    }

    pub(crate) fn pending_after_video_ack(
        &self,
        target: &str,
        assignments: &BrowserDeliveryStore,
    ) -> Result<Vec<PendingEvents>, TrajectoryStoreError> {
        if !exists(&self.root)? {
            return Ok(Vec::new());
        }
        check_private_path(&self.root, false)?;
        let mut output = Vec::new();
        for entry in fs::read_dir(&self.root).map_err(|_| TrajectoryStoreError::Io)? {
            let path = entry.map_err(|_| TrajectoryStoreError::Io)?.path();
            check_private_path(&path, false)?;
            if !path.is_dir() {
                continue;
            }
            let Some(ledger) = read_head(&path)? else {
                continue;
            };
            if ledger.retired
                || !ledger.descriptor.finalized
                || !ledger.video_receipt_accepted
                || ledger.event_receipt_accepted
                || ledger.video.is_none()
            {
                continue;
            }
            if archive_path(&self.root, &ledger.recording_id, &ledger.event_stream_id)? != path {
                return Err(TrajectoryStoreError::Invalid);
            }
            if let Ok(item) = self.authorized(
                &ledger.recording_id,
                &ledger.event_stream_id,
                target,
                assignments,
            ) {
                output.push(item);
                if output.len() == 16 {
                    break;
                }
            }
        }
        Ok(output)
    }

    /// Explicit startup recovery; normal discovery and read paths never repair state.
    pub(crate) fn recover_interrupted(&self) -> Result<(), TrajectoryStoreError> {
        if !exists(&self.root)? {
            return Ok(());
        }
        check_private_path(&self.root, false)?;
        for entry in fs::read_dir(&self.root).map_err(|_| TrajectoryStoreError::Io)? {
            let path = entry.map_err(|_| TrajectoryStoreError::Io)?.path();
            check_private_path(&path, false)?;
            if !path.is_dir() {
                continue;
            }
            if erasure_requested(&path)? {
                finish_erasure(&path)?;
                continue;
            }
            let Some(ledger) = read_head(&path)? else {
                continue;
            };
            if ledger.retired || ledger.descriptor.finalized {
                continue;
            }
            if archive_path(&self.root, &ledger.recording_id, &ledger.event_stream_id)? != path {
                return Err(TrajectoryStoreError::Invalid);
            }
            let mut archive = match EventsArchive::open(
                &self.root,
                &ledger.recording_id,
                &ledger.event_stream_id,
            ) {
                Ok(archive) => archive,
                Err(TrajectoryStoreError::Busy) => continue,
                Err(error) => return Err(error),
            };
            archive.recover_interrupted()?;
        }
        Ok(())
    }
}

fn exists(path: &Path) -> Result<bool, TrajectoryStoreError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(TrajectoryStoreError::Io),
    }
}

fn read_head(directory: &Path) -> Result<Option<EventsLedger>, TrajectoryStoreError> {
    if erasure_requested(directory)? {
        return Ok(None);
    }
    let path = directory.join("head.json");
    if !exists(&path)? {
        return Ok(None);
    }
    EventsLedger::read(&path).map(|(ledger, _cipher)| Some(ledger))
}

fn pending(ledger: EventsLedger) -> PendingEvents {
    PendingEvents {
        recording_id: ledger.recording_id,
        descriptor: ledger.descriptor,
        video: ledger.video,
        video_receipt_accepted: ledger.video_receipt_accepted,
        event_receipt_accepted: ledger.event_receipt_accepted,
        delivery_route: ledger.route,
    }
}
