//! Private attachment discovery survives release of the independently acknowledged video.

use super::history_files::{erasure_requested, finish_erasure};
use super::{
    browser_delivery::{BrowserDeliveryStore, BrowserMediaKind},
    trajectory_ledger::{TrajectoryLedger, check_private_path},
    trajectory_store::{
        TrajectoryArchive, TrajectoryStoreError, TrajectoryVideoBinding, archive_path,
    },
};
use crate::protocol::{DeliveryRoute, trajectory::CursorTrajectoryDescriptor};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub(crate) struct PendingTrajectory {
    pub(crate) recording_id: String,
    pub(crate) descriptor: CursorTrajectoryDescriptor,
    pub(crate) video: Option<TrajectoryVideoBinding>,
    pub(crate) video_receipt_accepted: bool,
    pub(crate) attachment_receipt_accepted: bool,
    pub(crate) delivery_route: DeliveryRoute,
}

#[derive(Clone, Debug)]
pub(crate) struct TrajectoryCatalog {
    root: PathBuf,
}

impl TrajectoryCatalog {
    pub(crate) fn erase_acknowledged(
        &self,
        recording: &str,
        source: &str,
    ) -> Result<(), TrajectoryStoreError> {
        let archive = TrajectoryArchive::open(&self.root, recording, source)?;
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
        trajectory: &str,
    ) -> Result<Option<PendingTrajectory>, TrajectoryStoreError> {
        let directory = archive_path(&self.root, recording, trajectory)?;
        if !exists(&directory)? {
            return Ok(None);
        }
        check_private_path(&self.root, false)?;
        check_private_path(&directory, false)?;
        let Some(ledger) = read_head(&directory)? else {
            return Ok(None);
        };
        if ledger.recording_id != recording || ledger.trajectory_id != trajectory {
            return Err(TrajectoryStoreError::Invalid);
        }
        Ok((!ledger.retired).then(|| pending(ledger)))
    }

    pub(crate) fn authorized(
        &self,
        recording: &str,
        trajectory: &str,
        target: &str,
        assignments: &BrowserDeliveryStore,
    ) -> Result<PendingTrajectory, TrajectoryStoreError> {
        if assignments
            .target(recording, BrowserMediaKind::Recording)
            .map_err(|_| TrajectoryStoreError::Invalid)?
            .as_deref()
            != Some(target)
        {
            return Err(TrajectoryStoreError::Invalid);
        }
        let item = self
            .lookup(recording, trajectory)?
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
        trajectory: &str,
        target: &str,
        assignments: &BrowserDeliveryStore,
    ) -> Result<TrajectoryArchive, TrajectoryStoreError> {
        self.authorized(recording, trajectory, target, assignments)?;
        TrajectoryArchive::read(&self.root, recording, trajectory)
    }

    pub(crate) fn authorized_mutation(
        &self,
        recording: &str,
        trajectory: &str,
        target: &str,
        assignments: &BrowserDeliveryStore,
    ) -> Result<TrajectoryArchive, TrajectoryStoreError> {
        self.authorized(recording, trajectory, target, assignments)?;
        TrajectoryArchive::open(&self.root, recording, trajectory)
    }

    pub(crate) fn pending_after_video_ack(
        &self,
        target: &str,
        assignments: &BrowserDeliveryStore,
    ) -> Result<Vec<PendingTrajectory>, TrajectoryStoreError> {
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
                || ledger.attachment_receipt_accepted
                || ledger.video.is_none()
            {
                continue;
            }
            if archive_path(&self.root, &ledger.recording_id, &ledger.trajectory_id)? != path {
                return Err(TrajectoryStoreError::Invalid);
            }
            if let Ok(item) = self.authorized(
                &ledger.recording_id,
                &ledger.trajectory_id,
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
            if archive_path(&self.root, &ledger.recording_id, &ledger.trajectory_id)? != path {
                return Err(TrajectoryStoreError::Invalid);
            }
            let mut archive = match TrajectoryArchive::open(
                &self.root,
                &ledger.recording_id,
                &ledger.trajectory_id,
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

fn read_head(directory: &Path) -> Result<Option<TrajectoryLedger>, TrajectoryStoreError> {
    if erasure_requested(directory)? {
        return Ok(None);
    }
    let path = directory.join("head.json");
    if !exists(&path)? {
        return Ok(None);
    }
    TrajectoryLedger::read(&path).map(|(ledger, _cipher)| Some(ledger))
}

fn pending(ledger: TrajectoryLedger) -> PendingTrajectory {
    PendingTrajectory {
        recording_id: ledger.recording_id,
        descriptor: ledger.descriptor,
        video: ledger.video,
        video_receipt_accepted: ledger.video_receipt_accepted,
        attachment_receipt_accepted: ledger.attachment_receipt_accepted,
        delivery_route: ledger.route,
    }
}
