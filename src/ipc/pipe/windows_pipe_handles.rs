//! Windows handle helpers for named-pipe adapters.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::{
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use windows::Win32::{
    Foundation::{CloseHandle, DUPLICATE_SAME_ACCESS, DuplicateHandle, HANDLE},
    System::{
        IO::CancelSynchronousIo,
        Threading::{GetCurrentProcess, GetCurrentThread},
    },
};

#[derive(Clone, Debug)]
pub(super) struct BlockingReadCancel {
    state: Arc<BlockingReadCancelState>,
}

#[derive(Debug)]
struct BlockingReadCancelState {
    cancelled: AtomicBool,
    thread: Mutex<Option<HANDLE>>,
}

pub(super) struct BlockingReadGuard {
    cancel: BlockingReadCancel,
}

impl BlockingReadCancel {
    pub(super) fn new() -> Self {
        Self {
            state: Arc::new(BlockingReadCancelState {
                cancelled: AtomicBool::new(false),
                thread: Mutex::new(None),
            }),
        }
    }

    pub(super) fn begin_read(&self) -> io::Result<BlockingReadGuard> {
        if self.state.cancelled.load(Ordering::Acquire) {
            return Err(cancelled_read_error());
        }
        // SAFETY: GetCurrentThread returns a pseudo-handle for the current thread only; it is
        // immediately duplicated into an owned handle before it is shared with the canceller.
        let thread = duplicate_handle_same_access(unsafe { GetCurrentThread() })?;
        *self
            .state
            .thread
            .lock()
            .map_err(|_| io::Error::other("pipe read cancel lock poisoned"))? = Some(thread);
        if self.state.cancelled.load(Ordering::Acquire) {
            self.cancel_pending_read();
        }
        Ok(BlockingReadGuard {
            cancel: self.clone(),
        })
    }

    pub(super) fn cancel_pending_read(&self) {
        self.state.cancelled.store(true, Ordering::Release);
        let thread = self.state.thread.lock().ok().and_then(|slot| *slot);
        if let Some(thread) = thread {
            // SAFETY: `thread` is a duplicated thread handle for a thread in this process. The call
            // only requests cancellation of pending synchronous IO owned by that thread.
            let _ = unsafe { CancelSynchronousIo(thread) };
        }
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.state.cancelled.load(Ordering::Acquire)
    }
}

impl Drop for BlockingReadGuard {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.cancel.state.thread.lock()
            && let Some(thread) = slot.take()
        {
            close_handle(thread);
        }
    }
}

pub(super) fn duplicate_handle_same_access(handle: HANDLE) -> io::Result<HANDLE> {
    // SAFETY: GetCurrentProcess returns a pseudo-handle valid in the current process and is only
    // used as the source/target process for DuplicateHandle.
    let process = unsafe { GetCurrentProcess() };
    let mut duplicate = HANDLE::default();
    // SAFETY: `handle` is owned in this process; DuplicateHandle writes a non-inheritable duplicate
    // for the same process with the same access mask into `duplicate`.
    unsafe {
        DuplicateHandle(
            process,
            handle,
            process,
            &mut duplicate,
            0,
            false,
            DUPLICATE_SAME_ACCESS,
        )
    }
    .map_err(|error| to_io_error(&error))?;
    Ok(duplicate)
}

pub(super) fn valid_handle(handle: HANDLE) -> io::Result<HANDLE> {
    if handle.is_invalid() {
        Err(io::Error::last_os_error())
    } else {
        Ok(handle)
    }
}

pub(super) fn close_handle(handle: HANDLE) {
    if !handle.is_invalid() {
        // SAFETY: the handle is owned by one RAII wrapper and is closed exactly once in Drop.
        let _ = unsafe { CloseHandle(handle) };
    }
}

pub(super) fn to_io_error(error: &windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error(error.code().0)
}

fn cancelled_read_error() -> io::Error {
    io::Error::new(io::ErrorKind::ConnectionAborted, "pipe read cancelled")
}
