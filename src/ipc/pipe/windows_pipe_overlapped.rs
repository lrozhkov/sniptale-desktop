//! Overlapped named-pipe operation helpers.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::{fmt, io};

use windows::{
    Win32::{
        Foundation::{
            ERROR_IO_PENDING, ERROR_PIPE_CONNECTED, ERROR_PIPE_LISTENING, HANDLE, WAIT_FAILED,
            WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        Storage::FileSystem::{ReadFile, WriteFile},
        System::{
            IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED},
            Pipes::ConnectNamedPipe,
            Threading::{CreateEventW, INFINITE, WaitForSingleObject},
        },
    },
    core::{HRESULT, PCWSTR},
};

use super::windows_pipe_handles::{BlockingReadCancel, close_handle, to_io_error};

const READ_CANCEL_POLL_MS: u32 = 25;

pub(super) struct PendingPipeConnect {
    event: OverlappedEvent,
    overlapped: Box<OVERLAPPED>,
}

impl fmt::Debug for PendingPipeConnect {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PendingPipeConnect")
            .field("event", &self.event)
            .finish_non_exhaustive()
    }
}

impl PendingPipeConnect {
    fn new() -> io::Result<Self> {
        let event = OverlappedEvent::new()?;
        let mut overlapped = Box::<OVERLAPPED>::default();
        overlapped.hEvent = event.handle();
        Ok(Self { event, overlapped })
    }

    fn overlapped_mut(&mut self) -> *mut OVERLAPPED {
        self.overlapped.as_mut()
    }

    fn overlapped(&self) -> *const OVERLAPPED {
        self.overlapped.as_ref()
    }
}

#[derive(Debug)]
struct OverlappedEvent {
    handle: HANDLE,
}

impl OverlappedEvent {
    fn new() -> io::Result<Self> {
        // SAFETY: no security attributes or name are supplied; the returned event handle is owned
        // by this RAII wrapper and used only as an OVERLAPPED completion event.
        let handle = unsafe { CreateEventW(None, true, false, PCWSTR::null()) }
            .map_err(|error| to_io_error(&error))?;
        Ok(Self { handle })
    }

    fn handle(&self) -> HANDLE {
        self.handle
    }
}

impl Drop for OverlappedEvent {
    fn drop(&mut self) {
        close_handle(self.handle);
    }
}

pub(super) fn try_connect_overlapped(
    handle: HANDLE,
    pending: &mut Option<PendingPipeConnect>,
) -> io::Result<bool> {
    if let Some(connect) = pending {
        // SAFETY: `connect.event` is a valid manual-reset event owned by the pending OVERLAPPED
        // state; zero timeout makes this a nonblocking poll.
        match unsafe { WaitForSingleObject(connect.event.handle(), 0) } {
            WAIT_OBJECT_0 => {
                finish_pending_operation(handle, connect.overlapped())?;
                *pending = None;
                return Ok(true);
            }
            WAIT_TIMEOUT => return Ok(false),
            WAIT_FAILED => return Err(io::Error::last_os_error()),
            other => {
                return Err(io::Error::other(format!(
                    "unexpected pipe connect wait result {}",
                    other.0
                )));
            }
        }
    }

    let mut connect = PendingPipeConnect::new()?;
    // SAFETY: `handle` is an overlapped named-pipe server handle; `connect.overlapped` and its
    // event remain alive in `pending` until completion is observed.
    match unsafe { ConnectNamedPipe(handle, Some(connect.overlapped_mut())) } {
        Ok(()) => Ok(true),
        Err(error) if is_win32_error(&error, ERROR_IO_PENDING.0) => {
            *pending = Some(connect);
            Ok(false)
        }
        Err(error) if is_win32_error(&error, ERROR_PIPE_CONNECTED.0) => Ok(true),
        Err(error) if is_win32_error(&error, ERROR_PIPE_LISTENING.0) => Ok(false),
        Err(error) => Err(to_io_error(&error)),
    }
}

pub(super) fn read_from_overlapped_handle(
    handle: HANDLE,
    buffer: &mut [u8],
    cancel: Option<&BlockingReadCancel>,
) -> io::Result<usize> {
    if cancel.is_some_and(BlockingReadCancel::is_cancelled) {
        return Err(cancelled_read_error());
    }
    let event = OverlappedEvent::new()?;
    let mut overlapped = OVERLAPPED {
        hEvent: event.handle(),
        ..OVERLAPPED::default()
    };
    // SAFETY: `handle` is an overlapped pipe handle; `buffer` and `overlapped` remain alive until
    // `finish_pending_read` returns and the operation is complete or cancelled.
    match unsafe { ReadFile(handle, Some(buffer), None, Some(&raw mut overlapped)) } {
        Ok(()) => finish_pending_read(handle, &overlapped, &event, cancel),
        Err(error) if is_win32_error(&error, ERROR_IO_PENDING.0) => {
            finish_pending_read(handle, &overlapped, &event, cancel)
        }
        Err(error) => Err(to_io_error(&error)),
    }
}

pub(super) fn write_to_overlapped_handle(handle: HANDLE, buffer: &[u8]) -> io::Result<usize> {
    let event = OverlappedEvent::new()?;
    let mut overlapped = OVERLAPPED {
        hEvent: event.handle(),
        ..OVERLAPPED::default()
    };
    // SAFETY: `handle` is an overlapped pipe handle; `buffer` and `overlapped` remain alive until
    // `finish_pending_write` returns and the operation is complete.
    match unsafe { WriteFile(handle, Some(buffer), None, Some(&raw mut overlapped)) } {
        Ok(()) => finish_pending_write(handle, &overlapped, &event),
        Err(error) if is_win32_error(&error, ERROR_IO_PENDING.0) => {
            finish_pending_write(handle, &overlapped, &event)
        }
        Err(error) => Err(to_io_error(&error)),
    }
}

fn finish_pending_read(
    handle: HANDLE,
    overlapped: &OVERLAPPED,
    event: &OverlappedEvent,
    cancel: Option<&BlockingReadCancel>,
) -> io::Result<usize> {
    loop {
        if cancel.is_some_and(BlockingReadCancel::is_cancelled) {
            // SAFETY: `overlapped` belongs to the pending read on `handle` and remains alive until
            // `GetOverlappedResult` below observes cancellation/completion.
            let _ = unsafe { CancelIoEx(handle, Some(overlapped)) };
            let mut ignored = 0;
            // SAFETY: waiting here drains the cancelled operation before stack state is dropped.
            let _ = unsafe { GetOverlappedResult(handle, overlapped, &raw mut ignored, true) };
            return Err(cancelled_read_error());
        }
        match wait_for_event(event, READ_CANCEL_POLL_MS)? {
            WaitState::Completed => return finish_pending_operation(handle, overlapped),
            WaitState::Pending => continue,
        }
    }
}

fn finish_pending_write(
    handle: HANDLE,
    overlapped: &OVERLAPPED,
    event: &OverlappedEvent,
) -> io::Result<usize> {
    wait_for_event(event, INFINITE)?;
    finish_pending_operation(handle, overlapped)
}

fn finish_pending_operation(handle: HANDLE, overlapped: *const OVERLAPPED) -> io::Result<usize> {
    let mut bytes_transferred = 0;
    // SAFETY: the caller keeps `overlapped` alive until the OS reports completion for `handle`.
    unsafe { GetOverlappedResult(handle, overlapped, &raw mut bytes_transferred, false) }
        .map_err(|error| to_io_error(&error))?;
    Ok(bytes_transferred as usize)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WaitState {
    Completed,
    Pending,
}

fn wait_for_event(event: &OverlappedEvent, timeout_ms: u32) -> io::Result<WaitState> {
    // SAFETY: `event` is a valid event handle owned by an active OVERLAPPED operation.
    match unsafe { WaitForSingleObject(event.handle(), timeout_ms) } {
        WAIT_OBJECT_0 => Ok(WaitState::Completed),
        WAIT_TIMEOUT => Ok(WaitState::Pending),
        WAIT_FAILED => Err(io::Error::last_os_error()),
        other => Err(io::Error::other(format!(
            "unexpected pipe overlapped wait result {}",
            other.0
        ))),
    }
}

fn is_win32_error(error: &windows::core::Error, code: u32) -> bool {
    error.code() == HRESULT::from_win32(code)
}

fn cancelled_read_error() -> io::Error {
    io::Error::new(io::ErrorKind::ConnectionAborted, "pipe read cancelled")
}
