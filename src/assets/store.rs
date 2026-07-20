//! Private crash-recovery persistence for Manual publication jobs.

use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
};

use super::{AssetJob, AssetJobId};

const MAX_ASSET_JOB_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug)]
pub(crate) struct AssetJobStore {
    root: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AssetJobStoreError {
    Io,
    InvalidJob,
    Oversize,
}

impl AssetJobStore {
    pub(crate) fn open(root: PathBuf) -> Result<Self, AssetJobStoreError> {
        fs::create_dir_all(&root).map_err(|_| AssetJobStoreError::Io)?;
        let metadata = fs::symlink_metadata(&root).map_err(|_| AssetJobStoreError::Io)?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err(AssetJobStoreError::Io);
        }
        Ok(Self { root })
    }

    pub(crate) fn save(&self, job: &AssetJob) -> Result<(), AssetJobStoreError> {
        job.validate().map_err(|_| AssetJobStoreError::InvalidJob)?;
        let payload = serde_json::to_vec(job).map_err(|_| AssetJobStoreError::InvalidJob)?;
        if payload.len() > MAX_ASSET_JOB_BYTES {
            return Err(AssetJobStoreError::Oversize);
        }
        let target = self.path_for(job.id());
        let pending = self.pending_path_for(job.id());
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)
            .map_err(|_| AssetJobStoreError::Io)?;
        if file.write_all(&payload).is_err() || file.sync_all().is_err() {
            let _ = fs::remove_file(&pending);
            return Err(AssetJobStoreError::Io);
        }
        fs::rename(&pending, target).map_err(|_| AssetJobStoreError::Io)
    }

    pub(crate) fn load(&self, id: &AssetJobId) -> Result<Option<AssetJob>, AssetJobStoreError> {
        let path = self.path_for(id);
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(AssetJobStoreError::Io),
        };
        let mut payload = Vec::with_capacity(1024);
        file.take((MAX_ASSET_JOB_BYTES + 1) as u64)
            .read_to_end(&mut payload)
            .map_err(|_| AssetJobStoreError::Io)?;
        if payload.len() > MAX_ASSET_JOB_BYTES {
            return Err(AssetJobStoreError::Oversize);
        }
        let job = serde_json::from_slice::<AssetJob>(&payload)
            .map_err(|_| AssetJobStoreError::InvalidJob)?;
        job.validate().map_err(|_| AssetJobStoreError::InvalidJob)?;
        Ok(Some(job))
    }

    fn path_for(&self, id: &AssetJobId) -> PathBuf {
        self.root.join(format!("{}.json", id.as_str()))
    }

    fn pending_path_for(&self, id: &AssetJobId) -> PathBuf {
        self.root.join(format!("{}.json.pending", id.as_str()))
    }
}
