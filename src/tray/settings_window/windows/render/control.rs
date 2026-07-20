//! Modern owner-drawn chrome for accessible native settings controls.
#![allow(unsafe_code)]

use windows::Win32::{
    Foundation::{LPARAM, RECT, WPARAM},
    Graphics::Gdi::{
        CreatePen, CreateSolidBrush, DRAW_TEXT_FORMAT, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT,
        DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteObject, DrawTextW, FillRect, GetStockObject,
        HGDIOBJ, HOLLOW_BRUSH, PS_SOLID, RoundRect, SelectObject, SetBkMode, SetTextColor,
        TRANSPARENT,
    },
    UI::{
        Controls::{
            DRAWITEMSTRUCT, ODS_COMBOBOXEDIT, ODS_DISABLED, ODS_FOCUS, ODS_HOTLIGHT, ODS_SELECTED,
            ODT_COMBOBOX,
        },
        HiDpi::GetDpiForWindow,
        WindowsAndMessaging::{
            CB_GETLBTEXT, CB_GETLBTEXTLEN, GetWindowTextLengthW, GetWindowTextW, SendMessageW,
        },
    },
};

use super::geometry::{color_ref, has_state, inset, inset_horizontal, scale};
use crate::tray::settings_window::{
    control_chrome::{ButtonVisual, ControlTextAlignment, button_chrome},
    palette::{Color, ThemePalette},
};

pub(in crate::tray::settings_window::windows) fn draw_control(
    item: &DRAWITEMSTRUCT,
    palette: ThemePalette,
    visual: ButtonVisual,
) {
    if item.CtlType == ODT_COMBOBOX {
        draw_combo(item, palette);
    } else {
        draw_button(item, palette, visual);
    }
}

fn draw_button(item: &DRAWITEMSTRUCT, palette: ThemePalette, visual: ButtonVisual) {
    let disabled = has_state(item, ODS_DISABLED.0);
    let pressed = has_state(item, ODS_SELECTED.0);
    let hot = has_state(item, ODS_HOTLIGHT.0);
    let chrome = button_chrome(palette, visual, disabled, pressed || hot);
    let align = match chrome.alignment {
        ControlTextAlignment::Left => DT_LEFT,
        ControlTextAlignment::Center => DT_CENTER,
    };
    paint_rounded(item, chrome.background, chrome.border, 8, 1);
    draw_item_text(
        item,
        chrome.foreground,
        align,
        if chrome.alignment == ControlTextAlignment::Left {
            14
        } else {
            8
        },
    );
    draw_focus(item, palette.accent, 8);
}

fn draw_combo(item: &DRAWITEMSTRUCT, palette: ThemePalette) {
    let disabled = has_state(item, ODS_DISABLED.0);
    let selected = has_state(item, ODS_SELECTED.0);
    let edit = has_state(item, ODS_COMBOBOXEDIT.0);
    let opaque = palette.opaque_controls();
    let background = if selected && !edit {
        opaque.accent_soft
    } else {
        opaque.input
    };
    let foreground = if disabled {
        palette.text_muted
    } else if selected && !edit {
        palette.accent
    } else {
        palette.text
    };
    // SAFETY: WM_DRAWITEM supplies a live HDC and rectangle for the synchronous combo draw.
    unsafe {
        let raw_brush = CreateSolidBrush(color_ref(background));
        let _brush = GdiObject(raw_brush.into());
        let _ = FillRect(item.hDC, &item.rcItem, raw_brush);
    }
    let text = combo_text(item);
    draw_text_buffer(
        item,
        inset_horizontal(item.rcItem, scale(10, dpi(item))),
        foreground,
        DT_LEFT,
        text,
    );
}

fn draw_item_text(item: &DRAWITEMSTRUCT, color: Color, align: DRAW_TEXT_FORMAT, inset: i32) {
    let text = window_text(item);
    draw_text_buffer(
        item,
        inset_horizontal(item.rcItem, scale(inset, dpi(item))),
        color,
        align,
        text,
    );
}

fn draw_text_buffer(
    item: &DRAWITEMSTRUCT,
    mut rect: RECT,
    color: Color,
    align: DRAW_TEXT_FORMAT,
    mut text: Vec<u16>,
) {
    // SAFETY: text and RECT remain live for this synchronous GDI draw.
    unsafe {
        let _ = SetBkMode(item.hDC, TRANSPARENT);
        let _ = SetTextColor(item.hDC, color_ref(color));
        let _ = DrawTextW(
            item.hDC,
            &mut text,
            &mut rect,
            align | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
    }
}

fn paint_rounded(item: &DRAWITEMSTRUCT, background: Color, border: Color, radius: i32, width: i32) {
    paint_rounded_rect(item, item.rcItem, background, border, radius, width);
}

fn paint_rounded_rect(
    item: &DRAWITEMSTRUCT,
    rect: RECT,
    background: Color,
    border: Color,
    radius: i32,
    width: i32,
) {
    let dpi = dpi(item);
    // SAFETY: created GDI objects are selected only for this draw, restored, and owned by RAII.
    unsafe {
        let brush = GdiObject(CreateSolidBrush(color_ref(background)).into());
        let pen = GdiObject(CreatePen(PS_SOLID, scale(width, dpi), color_ref(border)).into());
        let old_brush = SelectObject(item.hDC, brush.0);
        let old_pen = SelectObject(item.hDC, pen.0);
        let corner = scale(radius * 2, dpi);
        let _ = RoundRect(
            item.hDC,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            corner,
            corner,
        );
        let _ = SelectObject(item.hDC, old_brush);
        let _ = SelectObject(item.hDC, old_pen);
    }
}

fn draw_focus(item: &DRAWITEMSTRUCT, color: Color, radius: i32) {
    if !has_state(item, ODS_FOCUS.0) {
        return;
    }
    let dpi = dpi(item);
    let rect = inset(item.rcItem, scale(2, dpi));
    // SAFETY: stock hollow brush is borrowed; the owned pen is restored before release.
    unsafe {
        let pen = GdiObject(CreatePen(PS_SOLID, scale(2, dpi), color_ref(color)).into());
        let old_brush = SelectObject(item.hDC, GetStockObject(HOLLOW_BRUSH));
        let old_pen = SelectObject(item.hDC, pen.0);
        let corner = scale(radius * 2, dpi);
        let _ = RoundRect(
            item.hDC,
            rect.left,
            rect.top,
            rect.right,
            rect.bottom,
            corner,
            corner,
        );
        let _ = SelectObject(item.hDC, old_brush);
        let _ = SelectObject(item.hDC, old_pen);
    }
}

fn window_text(item: &DRAWITEMSTRUCT) -> Vec<u16> {
    // SAFETY: the child HWND remains live during its synchronous owner draw.
    unsafe {
        let length = GetWindowTextLengthW(item.hwndItem).max(0);
        let mut text = vec![0_u16; usize::try_from(length).unwrap_or(0).saturating_add(1)];
        let copied = GetWindowTextW(item.hwndItem, &mut text).max(0) as usize;
        text.truncate(copied);
        text
    }
}

fn combo_text(item: &DRAWITEMSTRUCT) -> Vec<u16> {
    if item.itemID == u32::MAX {
        return window_text(item);
    }
    // SAFETY: combo item strings were inserted with CBS_HASSTRINGS and are copied synchronously.
    unsafe {
        let index = item.itemID as usize;
        let length = SendMessageW(item.hwndItem, CB_GETLBTEXTLEN, Some(WPARAM(index)), None)
            .0
            .max(0);
        let mut text = vec![0_u16; length as usize + 1];
        let copied = SendMessageW(
            item.hwndItem,
            CB_GETLBTEXT,
            Some(WPARAM(index)),
            Some(LPARAM(text.as_mut_ptr() as isize)),
        )
        .0
        .max(0) as usize;
        text.truncate(copied);
        text
    }
}

#[derive(Debug)]
struct GdiObject(HGDIOBJ);

impl Drop for GdiObject {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: this wrapper owns an unselected GDI brush or pen and deletes it once.
            let _ = unsafe { DeleteObject(self.0) };
            self.0 = HGDIOBJ::default();
        }
    }
}

fn dpi(item: &DRAWITEMSTRUCT) -> u32 {
    // SAFETY: querying DPI retains no pointers and the owner-drawn child HWND is live.
    unsafe { GetDpiForWindow(item.hwndItem) }.max(96)
}
