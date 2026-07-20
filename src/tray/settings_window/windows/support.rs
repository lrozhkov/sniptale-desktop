//! Small value conversions shared by settings Win32 adapters.

use std::{ffi::OsStr, io, os::windows::ffi::OsStrExt};

pub(super) fn wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain([0]).collect()
}

pub(super) fn to_io_error(error: windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error(error.code().0)
}
