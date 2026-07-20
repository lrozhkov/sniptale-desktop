//! Raw Win32 child-control, font, and scroll operations.
#![allow(unsafe_code)]

use super::*;
use windows::Win32::{
    Foundation::{COLORREF, RECT},
    Graphics::Gdi::{
        CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH,
        DeleteObject, FW_NORMAL, FW_SEMIBOLD, GetStockObject, HDC, HFONT, HOLLOW_BRUSH,
        InvalidateRect, OUT_DEFAULT_PRECIS, SetBkColor, SetBkMode, SetTextColor, TRANSPARENT,
    },
    UI::{
        Controls::{SetScrollInfo, SetWindowTheme, ShowScrollBar},
        Input::KeyboardAndMouse::{EnableWindow, SetFocus},
        WindowsAndMessaging::{
            BM_SETCHECK, CB_ADDSTRING, CB_RESETCONTENT, CB_SETCURSEL, CB_SETITEMHEIGHT,
            GetClientRect, HMENU, SB_VERT, SCROLLINFO, SIF_PAGE, SIF_POS, SIF_RANGE, SW_HIDE,
            SW_SHOWNORMAL, SendMessageW, SetWindowPos, SetWindowTextW, ShowWindow, WINDOW_STYLE,
            WM_SETFONT, WS_CHILD, WS_VISIBLE,
        },
    },
};

use crate::tray::settings_window::{
    layout::{DipRect, SettingsLayout},
    palette::Color,
};

#[derive(Debug)]
pub(super) struct NativeFonts {
    normal: HFONT,
    title: HFONT,
    section: HFONT,
}

impl NativeFonts {
    pub(super) fn create(dpi: u32) -> Self {
        Self {
            normal: font(dpi, 10, FW_NORMAL.0 as i32),
            title: font(dpi, 18, FW_SEMIBOLD.0 as i32),
            section: font(dpi, 15, FW_SEMIBOLD.0 as i32),
        }
    }

    pub(super) fn apply(&self, all: &[HWND], titles: &[HWND]) {
        apply_font(all, self.normal);
        apply_font(titles, self.section);
        if let Some(headline) = titles.first() {
            apply_font(&[*headline], self.title);
        }
    }
}

impl Drop for NativeFonts {
    fn drop(&mut self) {
        for font in [self.normal, self.title, self.section] {
            if !font.is_invalid() {
                // SAFETY: these GDI fonts are owned by this RAII set and released once.
                let _ = unsafe { DeleteObject(font.into()) };
            }
        }
    }
}

pub(super) fn create_control(
    class: &str,
    text: &str,
    extra_style: u32,
    parent: HWND,
    id: usize,
) -> io::Result<HWND> {
    let class = wide_null(class);
    let text = wide_null(text);
    // SAFETY: strings are NUL-terminated; the parent owns the child HWND lifetime.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class.as_ptr()),
            PCWSTR(text.as_ptr()),
            WS_CHILD | WS_VISIBLE | WINDOW_STYLE(extra_style),
            0,
            0,
            1,
            1,
            Some(parent),
            (id != 0).then(|| HMENU(id as *mut _)),
            None,
            None,
        )
    }
    .map_err(to_io_error)
}

pub(super) fn set_text(handle: HWND, value: &str) -> io::Result<()> {
    let value = wide_null(value);
    // SAFETY: the NUL-terminated text is borrowed only for this synchronous call.
    unsafe { SetWindowTextW(handle, PCWSTR(value.as_ptr())) }.map_err(to_io_error)
}

pub(super) fn set_visible(handle: HWND, visible: bool) {
    // SAFETY: this changes only the visibility of an adapter-owned child HWND.
    let _ = unsafe { ShowWindow(handle, if visible { SW_SHOWNORMAL } else { SW_HIDE }) };
}

pub(super) fn set_enabled(handle: HWND, enabled: bool) {
    // SAFETY: this changes only the enabled state of an adapter-owned child HWND.
    let _ = unsafe { EnableWindow(handle, enabled) };
}

pub(super) fn set_checked(handle: HWND, state: usize) {
    // SAFETY: BM_SETCHECK synchronously updates an adapter-owned native checkbox.
    unsafe { SendMessageW(handle, BM_SETCHECK, Some(WPARAM(state)), None) };
}

pub(super) fn set_combo_metrics(handles: &[HWND], dpi: u32) {
    let scale = |value: i32| value.saturating_mul(dpi as i32) / 96;
    for handle in handles {
        // SAFETY: CB_SETITEMHEIGHT synchronously updates adapter-owned fixed-height combo items.
        unsafe {
            SendMessageW(
                *handle,
                CB_SETITEMHEIGHT,
                Some(WPARAM(usize::MAX)),
                Some(LPARAM(scale(34) as isize)),
            );
            SendMessageW(
                *handle,
                CB_SETITEMHEIGHT,
                Some(WPARAM(0)),
                Some(LPARAM(scale(32) as isize)),
            );
        }
    }
}

pub(super) fn set_combo_items(handle: HWND, labels: &[&str], selected: usize) {
    // SAFETY: every message is synchronous and temporary UTF-16 strings outlive its call.
    unsafe { SendMessageW(handle, CB_RESETCONTENT, None, None) };
    for label in labels {
        let label = wide_null(label);
        unsafe {
            SendMessageW(
                handle,
                CB_ADDSTRING,
                None,
                Some(LPARAM(label.as_ptr() as isize)),
            )
        };
    }
    unsafe { SendMessageW(handle, CB_SETCURSEL, Some(WPARAM(selected)), None) };
}

pub(super) fn move_dip(handle: HWND, rect: DipRect, dpi: u32) {
    let scale = |value: i32| value.saturating_mul(dpi as i32) / 96;
    // SAFETY: the child HWND is live and no z-order/activation change is requested.
    let _ = unsafe {
        SetWindowPos(
            handle,
            None,
            scale(rect.left),
            scale(rect.top),
            scale(rect.width.max(1)),
            scale(rect.height.max(1)),
            windows::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE
                | windows::Win32::UI::WindowsAndMessaging::SWP_NOZORDER,
        )
    };
}

pub(super) fn client_size(handle: HWND) -> (u32, u32) {
    let mut rect = RECT::default();
    // SAFETY: GetClientRect writes one initialized RECT for this live HWND.
    if unsafe { GetClientRect(handle, &mut rect) }.is_ok() {
        (
            u32::try_from(rect.right.max(0)).unwrap_or(0),
            u32::try_from(rect.bottom.max(0)).unwrap_or(0),
        )
    } else {
        (1, 1)
    }
}

pub(super) fn update_scrollbar(handle: HWND, layout: SettingsLayout, scroll: i32) {
    let mut info = SCROLLINFO {
        cbSize: std::mem::size_of::<SCROLLINFO>() as u32,
        fMask: SIF_RANGE | SIF_PAGE | SIF_POS,
        nMin: 0,
        nMax: layout.content_height + crate::tray::settings_window::layout::PAGE_GUTTER * 2,
        nPage: u32::try_from(layout.viewport.height).unwrap_or(0),
        nPos: scroll,
        ..SCROLLINFO::default()
    };
    // SAFETY: the initialized SCROLLINFO is read synchronously by the live settings HWND.
    unsafe {
        SetScrollInfo(handle, SB_VERT, &raw mut info, true);
        let _ = ShowScrollBar(handle, SB_VERT, layout.max_scroll > 0);
    }
}

pub(super) fn apply_control_theme(handles: &[HWND], dark: bool) {
    let name = wide_null(if dark {
        "DarkMode_Explorer"
    } else {
        "Explorer"
    });
    for handle in handles {
        // SAFETY: theme strings are NUL-terminated and no pointer is retained.
        let _ = unsafe { SetWindowTheme(*handle, PCWSTR(name.as_ptr()), PCWSTR::null()) };
    }
}

pub(super) fn static_color(hdc: HDC, color: Color) -> LRESULT {
    // SAFETY: HDC is valid only during WM_CTLCOLORSTATIC; system hollow brush is borrowed.
    unsafe {
        let _ = SetBkMode(hdc, TRANSPARENT);
        let _ = SetTextColor(hdc, color_ref(color));
        LRESULT(GetStockObject(HOLLOW_BRUSH).0 as isize)
    }
}

pub(super) fn checkbox_color(
    hdc: HDC,
    text: Color,
    background: Color,
    brush: &ControlBackgroundBrush,
) -> LRESULT {
    // SAFETY: HDC and the adapter-owned brush are valid for this synchronous WM_CTLCOLORBTN call.
    unsafe {
        let _ = SetBkMode(hdc, TRANSPARENT);
        let _ = SetBkColor(hdc, color_ref(background));
        let _ = SetTextColor(hdc, color_ref(text));
        brush.as_lresult()
    }
}

pub(super) fn invalidate(handle: HWND) {
    // SAFETY: invalidating the live settings client retains no pointers.
    let _ = unsafe { InvalidateRect(Some(handle), None, false) };
}

pub(super) fn focus(handle: HWND) {
    // SAFETY: focus is moved to a live adapter-owned HWND after a user action.
    let _ = unsafe { SetFocus(Some(handle)) };
}

fn font(dpi: u32, points: i32, weight: i32) -> HFONT {
    let face = wide_null("Segoe UI Variable Text");
    let height = -(points.saturating_mul(dpi as i32) / 72).max(1);
    // SAFETY: CreateFontW copies the face name and returns an owned GDI object.
    unsafe {
        CreateFontW(
            height,
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            u32::from(DEFAULT_PITCH.0),
            PCWSTR(face.as_ptr()),
        )
    }
}

fn apply_font(handles: &[HWND], font: HFONT) {
    for handle in handles {
        // SAFETY: the font outlives every child HWND receiving this synchronous message.
        unsafe {
            SendMessageW(
                *handle,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            )
        };
    }
}

pub(super) const fn color_ref(color: Color) -> COLORREF {
    COLORREF(color.red as u32 | ((color.green as u32) << 8) | ((color.blue as u32) << 16))
}
