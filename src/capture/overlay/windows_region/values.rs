#![cfg(windows)]

use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

use windows::Win32::Foundation::LPARAM;

use crate::capture::coordinates::{PhysicalPoint, PhysicalRect};

pub(super) fn mouse_point(bounds: PhysicalRect, lparam: LPARAM) -> PhysicalPoint {
    let bytes = lparam.0.to_le_bytes();
    let x = i16::from_le_bytes([bytes[0], bytes[1]]);
    let y = i16::from_le_bytes([bytes[2], bytes[3]]);
    PhysicalPoint {
        x: bounds.x + i32::from(x),
        y: bounds.y + i32::from(y),
    }
}

pub(super) fn wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}
