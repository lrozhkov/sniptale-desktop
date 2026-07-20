//! Windows named-pipe handle adapter.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::{
    ffi::OsStr,
    io::{self, Read, Write},
    os::windows::ffi::OsStrExt,
};

#[cfg(all(windows, not(test)))]
use windows::Win32::{
    Foundation::{GENERIC_READ, GENERIC_WRITE},
    Storage::FileSystem::{CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_MODE, OPEN_EXISTING},
};
use windows::{
    Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{FILE_FLAG_OVERLAPPED, PIPE_ACCESS_DUPLEX},
        System::Pipes::{
            CreateNamedPipeW, NAMED_PIPE_MODE, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_TYPE_BYTE, PIPE_WAIT, SetNamedPipeHandleState,
        },
    },
    core::PCWSTR,
};

use super::windows_pipe_peer::WindowsPipePeerIdentity;
use super::{
    name::PipeEndpointName,
    windows_pipe_handles::{
        BlockingReadCancel, close_handle, duplicate_handle_same_access, to_io_error, valid_handle,
    },
    windows_pipe_overlapped::{
        PendingPipeConnect, read_from_overlapped_handle, try_connect_overlapped,
        write_to_overlapped_handle,
    },
    windows_pipe_security::WindowsPipeSecurityAttributes,
};
use crate::protocol::{EXTENSION_TO_HOST_MAX_MESSAGE_BYTES, HOST_TO_EXTENSION_MAX_MESSAGE_BYTES};

#[derive(Debug)]
pub(crate) struct WindowsPipe {
    handle: HANDLE,
    pending_connect: Option<PendingPipeConnect>,
}

#[derive(Debug)]
pub(crate) struct WindowsPipeReader {
    handle: HANDLE,
    read_cancel: BlockingReadCancel,
}

#[derive(Debug)]
pub(crate) struct WindowsPipeWriter {
    handle: HANDLE,
}

#[derive(Clone, Debug)]
pub(crate) struct WindowsPipeShutdown {
    read_cancel: BlockingReadCancel,
}

// SAFETY: Windows HANDLE values are process-wide references. Each wrapper owns one pipe handle,
// moves ownership as a whole, and closes it exactly once; split handles are duplicated owners.
unsafe impl Send for WindowsPipe {}
unsafe impl Send for WindowsPipeReader {}
unsafe impl Send for WindowsPipeWriter {}
unsafe impl Send for WindowsPipeShutdown {}

impl WindowsPipe {
    pub(crate) fn create_server_nonblocking(
        endpoint: &PipeEndpointName,
        current_user_sid: &str,
        max_instances: u32,
    ) -> io::Result<Self> {
        Self::create_server_with_wait_mode(endpoint, current_user_sid, PIPE_WAIT, max_instances)
    }

    fn create_server_with_wait_mode(
        endpoint: &PipeEndpointName,
        current_user_sid: &str,
        wait_mode: NAMED_PIPE_MODE,
        max_instances: u32,
    ) -> io::Result<Self> {
        let name = wide_null(endpoint.as_str());
        let security = WindowsPipeSecurityAttributes::new(current_user_sid)?;
        let open_mode = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
        let pipe_mode =
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_REJECT_REMOTE_CLIENTS | wait_mode;
        let outbound_size = u32::try_from(HOST_TO_EXTENSION_MAX_MESSAGE_BYTES)
            .map_err(|_| io::Error::other("host message limit exceeds u32"))?;
        let inbound_size = u32::try_from(EXTENSION_TO_HOST_MAX_MESSAGE_BYTES)
            .map_err(|_| io::Error::other("extension message limit exceeds u32"))?;
        // SAFETY: `name` is NUL-terminated and lives for the call; security attributes point to a
        // current-user-only descriptor owned by `security` for the duration of this call; the
        // returned overlapped raw handle is immediately wrapped in RAII.
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                open_mode,
                pipe_mode,
                max_instances,
                outbound_size,
                inbound_size,
                0,
                Some(security.as_ptr()),
            )
        };
        Self::from_handle(handle)
    }

    #[cfg(all(windows, not(test)))]
    pub(crate) fn connect_client(endpoint: &PipeEndpointName) -> io::Result<Self> {
        let name = wide_null(endpoint.as_str());
        let access = GENERIC_READ.0 | GENERIC_WRITE.0;
        // SAFETY: `name` is NUL-terminated and lives for the call; this opens an existing pipe
        // endpoint without inheriting template handles or security attributes.
        let handle = unsafe {
            CreateFileW(
                PCWSTR(name.as_ptr()),
                access,
                FILE_SHARE_MODE(0),
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED,
                None,
            )
        }
        .map_err(|error| to_io_error(&error))?;
        Self::from_handle(handle)
    }

    pub(crate) fn try_wait_for_client(&mut self) -> io::Result<bool> {
        try_connect_overlapped(self.handle, &mut self.pending_connect)
    }

    pub(crate) fn set_blocking_wait_mode(&self) -> io::Result<()> {
        let mode = PIPE_WAIT;
        // SAFETY: `self.handle` is a valid named-pipe handle; `mode` lives for the call and only
        // changes the pipe wait mode back to blocking before session reads/writes begin.
        unsafe { SetNamedPipeHandleState(self.handle, Some(&raw const mode), None, None) }
            .map_err(|error| to_io_error(&error))
    }

    pub(crate) fn connected_client_identity(&self) -> io::Result<WindowsPipePeerIdentity> {
        super::windows_pipe_peer::connected_client_identity(self.handle)
    }

    pub(crate) fn split_with_shutdown(
        self,
    ) -> io::Result<(WindowsPipeReader, WindowsPipeWriter, WindowsPipeShutdown)> {
        let read_cancel = BlockingReadCancel::new();
        let reader = WindowsPipeReader::from_handle(
            duplicate_handle_same_access(self.handle)?,
            read_cancel.clone(),
        )?;
        let writer = WindowsPipeWriter::from_handle(duplicate_handle_same_access(self.handle)?)?;
        drop(self);
        let shutdown = WindowsPipeShutdown { read_cancel };
        Ok((reader, writer, shutdown))
    }

    fn from_handle(handle: HANDLE) -> io::Result<Self> {
        valid_handle(handle).map(|handle| Self {
            handle,
            pending_connect: None,
        })
    }
}

impl Read for WindowsPipe {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        read_from_overlapped_handle(self.handle, buffer, None)
    }
}

impl Write for WindowsPipe {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        write_to_overlapped_handle(self.handle, buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl WindowsPipeReader {
    fn from_handle(handle: HANDLE, read_cancel: BlockingReadCancel) -> io::Result<Self> {
        valid_handle(handle).map(|handle| Self {
            handle,
            read_cancel,
        })
    }
}

impl Read for WindowsPipeReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let _read_guard = self.read_cancel.begin_read()?;
        read_from_overlapped_handle(self.handle, buffer, Some(&self.read_cancel))
    }
}

impl Drop for WindowsPipeReader {
    fn drop(&mut self) {
        close_handle(self.handle);
    }
}

impl WindowsPipeWriter {
    fn from_handle(handle: HANDLE) -> io::Result<Self> {
        valid_handle(handle).map(|handle| Self { handle })
    }
}

impl Write for WindowsPipeWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        write_to_overlapped_handle(self.handle, buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for WindowsPipeWriter {
    fn drop(&mut self) {
        close_handle(self.handle);
    }
}

impl Drop for WindowsPipe {
    fn drop(&mut self) {
        close_handle(self.handle);
    }
}

impl WindowsPipeShutdown {
    pub(crate) fn cancel_pending_read(&self) {
        self.read_cancel.cancel_pending_read();
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain([0]).collect()
}
