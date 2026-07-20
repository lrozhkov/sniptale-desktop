//! Windows named-pipe peer identity adapter.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::io;

use windows::{
    Win32::{
        Foundation::HANDLE,
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
        System::Pipes::{GetNamedPipeClientProcessId, GetNamedPipeClientSessionId},
        System::Threading::{
            OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
            QueryFullProcessImageNameW,
        },
    },
    core::PWSTR,
};

use super::windows_pipe_handles::{close_handle, to_io_error, valid_handle};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WindowsPipePeerIdentity {
    pub(crate) process_id: u32,
    pub(crate) session_id: u32,
    pub(crate) image_path: String,
    pub(crate) parent_process_id: u32,
    pub(crate) parent_image_path: String,
}

pub(crate) fn connected_client_identity(handle: HANDLE) -> io::Result<WindowsPipePeerIdentity> {
    let process_id = named_pipe_client_process_id(handle)?;
    let parent_process_id = parent_process_id(process_id)?;
    Ok(WindowsPipePeerIdentity {
        process_id,
        session_id: named_pipe_client_session_id(handle)?,
        image_path: process_image_path(process_id)?,
        parent_process_id,
        parent_image_path: process_image_path(parent_process_id)?,
    })
}

fn named_pipe_client_process_id(handle: HANDLE) -> io::Result<u32> {
    let mut process_id = 0_u32;
    // SAFETY: `handle` is a connected named-pipe server handle and `process_id` is a valid out ptr.
    unsafe { GetNamedPipeClientProcessId(handle, &mut process_id) }
        .map_err(|error| to_io_error(&error))?;
    Ok(process_id)
}

fn named_pipe_client_session_id(handle: HANDLE) -> io::Result<u32> {
    let mut session_id = 0_u32;
    // SAFETY: `handle` is a connected named-pipe server handle and `session_id` is a valid out ptr.
    unsafe { GetNamedPipeClientSessionId(handle, &mut session_id) }
        .map_err(|error| to_io_error(&error))?;
    Ok(session_id)
}

fn process_image_path(process_id: u32) -> io::Result<String> {
    // SAFETY: OpenProcess receives a concrete PID observed from the connected pipe client and
    // requests query-only access without inheritable handles.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
        .map_err(|error| to_io_error(&error))
        .and_then(valid_handle)?;
    let result = query_process_image_path(process);
    close_handle(process);
    result
}

fn parent_process_id(process_id: u32) -> io::Result<u32> {
    // SAFETY: CreateToolhelp32Snapshot is called with the documented process-snapshot flag and no
    // target process id; the returned snapshot handle is immediately wrapped in RAII cleanup.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .map_err(|error| to_io_error(&error))
        .and_then(valid_handle)?;
    let result = parent_process_id_from_snapshot(snapshot, process_id);
    close_handle(snapshot);
    result
}

fn parent_process_id_from_snapshot(snapshot: HANDLE, process_id: u32) -> io::Result<u32> {
    let mut entry = PROCESSENTRY32W {
        dwSize: u32::try_from(std::mem::size_of::<PROCESSENTRY32W>())
            .map_err(|_| io::Error::other("process entry size exceeds u32"))?,
        ..PROCESSENTRY32W::default()
    };
    // SAFETY: `entry` points to writable PROCESSENTRY32W storage whose `dwSize` is initialized as
    // required by ToolHelp before the enumeration call.
    unsafe { Process32FirstW(snapshot, &mut entry) }.map_err(|error| to_io_error(&error))?;
    loop {
        if entry.th32ProcessID == process_id {
            return Ok(entry.th32ParentProcessID);
        }
        // SAFETY: `entry` remains valid writable storage for the next ToolHelp enumeration result.
        if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "pipe peer parent process not found",
            ));
        }
    }
}

fn query_process_image_path(process: HANDLE) -> io::Result<String> {
    let mut buffer = vec![0_u16; 32_768];
    let mut len = u32::try_from(buffer.len())
        .map_err(|_| io::Error::other("process image path buffer too large"))?;
    // SAFETY: `process` is an open process handle with query access; `buffer` is writable for
    // `len` UTF-16 code units and lives until the call returns.
    unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut len,
        )
    }
    .map_err(|error| to_io_error(&error))?;
    let len = usize::try_from(len).map_err(|_| io::Error::other("process image path too large"))?;
    Ok(String::from_utf16_lossy(&buffer[..len]))
}
