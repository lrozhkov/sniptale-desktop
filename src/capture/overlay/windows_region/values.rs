#![cfg(windows)]

use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

use windows::Win32::Foundation::LPARAM;

use crate::capture::coordinates::{PhysicalPoint, PhysicalRect};

pub(super) fn mouse_point(bounds: PhysicalRect, lparam: LPARAM) -> PhysicalPoint {
    let x = (lparam.0 & 0xffff) as u16 as i16;
    let y = ((lparam.0 >> 16) & 0xffff) as u16 as i16;
    PhysicalPoint {
        x: bounds.x + i32::from(x),
        y: bounds.y + i32::from(y),
    }
}

pub(super) fn wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}
