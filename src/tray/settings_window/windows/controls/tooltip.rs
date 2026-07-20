//! RAII tooltip for ellipsized destination paths.
#![allow(unsafe_code)]

use std::io;

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM},
        UI::{
            Controls::{
                TOOLTIPS_CLASS, TTF_IDISHWND, TTF_SUBCLASS, TTM_ADDTOOLW, TTM_UPDATETIPTEXTW,
                TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW,
            },
            WindowsAndMessaging::{
                CreateWindowExW, DestroyWindow, SendMessageW, WINDOW_EX_STYLE, WINDOW_STYLE,
            },
        },
    },
    core::{PCWSTR, PWSTR},
};

#[derive(Debug)]
pub(super) struct NativeTooltip {
    handle: HWND,
    owner: HWND,
    tool: HWND,
    text: Box<[u16]>,
}

impl NativeTooltip {
    pub(super) fn create(owner: HWND, tool: HWND, text: &str) -> io::Result<Self> {
        // SAFETY: the standard tooltip class copies creation values; owner stays live longer.
        let handle = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                TOOLTIPS_CLASS,
                PCWSTR::null(),
                WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
                0,
                0,
                0,
                0,
                Some(owner),
                None,
                None,
                None,
            )
        }
        .map_err(super::super::support::to_io_error)?;
        let mut tooltip = Self {
            handle,
            owner,
            tool,
            text: wide_box(text),
        };
        tooltip.add()?;
        Ok(tooltip)
    }

    pub(super) fn update(&mut self, text: &str) {
        self.text = wide_box(text);
        let mut info = self.tool_info();
        // SAFETY: the tooltip and stable boxed text remain live until the next update or Drop.
        unsafe {
            SendMessageW(
                self.handle,
                TTM_UPDATETIPTEXTW,
                None,
                Some(LPARAM((&raw mut info) as isize)),
            )
        };
    }

    fn add(&mut self) -> io::Result<()> {
        let mut info = self.tool_info();
        // SAFETY: tool info and boxed text are valid; TTF_SUBCLASS installs native tracking.
        let result = unsafe {
            SendMessageW(
                self.handle,
                TTM_ADDTOOLW,
                None,
                Some(LPARAM((&raw mut info) as isize)),
            )
        };
        (result.0 != 0)
            .then_some(())
            .ok_or_else(io::Error::last_os_error)
    }

    fn tool_info(&mut self) -> TTTOOLINFOW {
        TTTOOLINFOW {
            cbSize: std::mem::size_of::<TTTOOLINFOW>() as u32,
            uFlags: TTF_IDISHWND | TTF_SUBCLASS,
            hwnd: self.owner,
            uId: self.tool.0 as usize,
            lpszText: PWSTR(self.text.as_mut_ptr()),
            ..TTTOOLINFOW::default()
        }
    }
}

impl Drop for NativeTooltip {
    fn drop(&mut self) {
        if !self.handle.is_invalid() {
            // SAFETY: this owner created the tooltip HWND and releases it exactly once.
            let _ = unsafe { DestroyWindow(self.handle) };
        }
    }
}

fn wide_box(value: &str) -> Box<[u16]> {
    value
        .encode_utf16()
        .chain([0])
        .collect::<Vec<_>>()
        .into_boxed_slice()
}
