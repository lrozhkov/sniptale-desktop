//! Direct2D geometry conversion for the settings renderer.

use windows::Win32::{
    Foundation::{COLORREF, RECT},
    Graphics::Direct2D::{
        Common::{D2D_RECT_F, D2D1_COLOR_F},
        D2D1_ROUNDED_RECT,
    },
    UI::Controls::DRAWITEMSTRUCT,
};

use crate::tray::settings_window::{layout::DipRect, palette::Color};

pub(super) const fn color(value: Color) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: value.red as f32 / 255.0,
        g: value.green as f32 / 255.0,
        b: value.blue as f32 / 255.0,
        a: value.alpha as f32 / 255.0,
    }
}

pub(super) fn rect(value: DipRect) -> D2D_RECT_F {
    D2D_RECT_F {
        left: value.left as f32,
        top: value.top as f32,
        right: value.right() as f32,
        bottom: value.bottom() as f32,
    }
}

pub(super) fn rounded(value: DipRect, radius: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT {
        rect: rect(value),
        radiusX: radius,
        radiusY: radius,
    }
}

pub(super) const fn inset(rect: RECT, amount: i32) -> RECT {
    RECT {
        left: rect.left + amount,
        top: rect.top + amount,
        right: rect.right - amount,
        bottom: rect.bottom - amount,
    }
}

pub(super) const fn inset_horizontal(rect: RECT, amount: i32) -> RECT {
    RECT {
        left: rect.left + amount,
        right: rect.right - amount,
        ..rect
    }
}

pub(super) fn scale(value: i32, dpi: u32) -> i32 {
    value.saturating_mul(dpi as i32) / 96
}

pub(super) const fn color_ref(value: Color) -> COLORREF {
    COLORREF(value.red as u32 | ((value.green as u32) << 8) | ((value.blue as u32) << 16))
}

pub(super) const fn has_state(item: &DRAWITEMSTRUCT, flag: u32) -> bool {
    item.itemState.0 & flag != 0
}
