//! Windows disk-space probe for recording temp media.

use std::path::Path;

use super::super::disk_guard::{RecordingDiskGuardError, RecordingDiskSpace};

pub(crate) fn recording_volume_space(
    media_path: &Path,
) -> Result<RecordingDiskSpace, RecordingDiskGuardError> {
    platform_recording_volume_space(media_path)
}

#[cfg(windows)]
mod platform {
    #![allow(unsafe_code)]

    use std::{os::windows::ffi::OsStrExt, path::Path};

    use windows::{Win32::Storage::FileSystem::GetDiskFreeSpaceExW, core::PCWSTR};

    use super::{RecordingDiskGuardError, RecordingDiskSpace};

    pub(super) fn platform_recording_volume_space(
        media_path: &Path,
    ) -> Result<RecordingDiskSpace, RecordingDiskGuardError> {
        let directory = media_path.parent().unwrap_or(media_path);
        let mut path: Vec<u16> = directory.as_os_str().encode_wide().collect();
        path.push(0);

        let mut available_bytes = 0_u64;
        let mut capacity_bytes = 0_u64;
        let mut free_bytes = 0_u64;
        // SAFETY: `path` is a null-terminated UTF-16 directory path that lives for the call.
        // The out-pointers are valid mutable u64 slots and no raw pointers escape this adapter.
        unsafe {
            GetDiskFreeSpaceExW(
                PCWSTR(path.as_ptr()),
                Some(&mut available_bytes),
                Some(&mut capacity_bytes),
                Some(&mut free_bytes),
            )
        }
        .map_err(|_| RecordingDiskGuardError::ProbeFailed)?;
        Ok(RecordingDiskSpace {
            available_bytes,
            capacity_bytes,
        })
    }
}

#[cfg(windows)]
use platform::platform_recording_volume_space;
