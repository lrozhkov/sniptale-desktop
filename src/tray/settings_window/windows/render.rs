//! Direct2D/DirectWrite settings surface renderer.
#![allow(unsafe_code)]

use std::io;

use windows::{
    Win32::{
        Foundation::HWND,
        Graphics::{
            Direct2D::{
                Common::{D2D_RECT_F, D2D_SIZE_U},
                D2D1_ANTIALIAS_MODE_ALIASED, D2D1_DRAW_TEXT_OPTIONS_NONE,
                D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_HWND_RENDER_TARGET_PROPERTIES,
                D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES, D2D1CreateFactory,
                ID2D1Factory, ID2D1HwndRenderTarget,
            },
            DirectWrite::{
                DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL, DWriteCreateFactory,
                IDWriteFactory, IDWriteTextFormat,
            },
        },
    },
    core::PCWSTR,
};

#[path = "render/control.rs"]
mod control;
#[path = "render/geometry.rs"]
mod geometry;
use super::support::{to_io_error, wide_null};
pub(super) use control::draw_control;
use geometry::{color, rect, rounded};

use crate::tray::settings_window::{
    layout::{DipRect, PAGE_GUTTER, SettingsLayout, card_geometry},
    model::{SettingsWindowSection, SettingsWindowView},
    palette::{Color, ThemePalette},
};

#[derive(Debug)]
pub(super) struct SettingsRenderer {
    target: ID2D1HwndRenderTarget,
    brand_format: IDWriteTextFormat,
}

impl SettingsRenderer {
    pub(super) fn create(window: HWND, width: u32, height: u32, dpi: u32) -> io::Result<Self> {
        // SAFETY: factory creation returns an owned COM interface with no retained Rust pointers.
        let factory: ID2D1Factory =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }
                .map_err(to_io_error)?;
        let properties = D2D1_RENDER_TARGET_PROPERTIES {
            dpiX: dpi as f32,
            dpiY: dpi as f32,
            ..D2D1_RENDER_TARGET_PROPERTIES::default()
        };
        let window_properties = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd: window,
            pixelSize: D2D_SIZE_U { width, height },
            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
        };
        // SAFETY: both property structs are initialized and the adapter-owned HWND remains live.
        let target = unsafe {
            factory.CreateHwndRenderTarget(&raw const properties, &raw const window_properties)
        }
        .map_err(to_io_error)?;
        // SAFETY: the factory and returned text format are COM-owned; strings live for the call.
        let write_factory: IDWriteFactory =
            unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }.map_err(to_io_error)?;
        let family = wide_null("Segoe UI Variable Display");
        let locale = wide_null("en-us");
        let brand_format = unsafe {
            write_factory.CreateTextFormat(
                PCWSTR(family.as_ptr()),
                None,
                DWRITE_FONT_WEIGHT_SEMI_BOLD,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                18.0,
                PCWSTR(locale.as_ptr()),
            )
        }
        .map_err(to_io_error)?;
        Ok(Self {
            target,
            brand_format,
        })
    }

    pub(super) fn resize(&self, width: u32, height: u32, dpi: u32) -> io::Result<()> {
        let size = D2D_SIZE_U { width, height };
        // SAFETY: the render target remains bound to the same live HWND and size is initialized.
        unsafe {
            self.target.Resize(&raw const size)?;
            self.target.SetDpi(dpi as f32, dpi as f32);
        }
        Ok(())
    }

    pub(super) fn resize_recovering(
        &mut self,
        window: HWND,
        width: u32,
        height: u32,
        dpi: u32,
    ) -> io::Result<()> {
        if self.resize(width, height, dpi).is_err() {
            *self = Self::create(window, width, height, dpi)?;
        }
        Ok(())
    }

    pub(super) fn paint_recovering(
        &mut self,
        window: HWND,
        size: (u32, u32),
        dpi: u32,
        palette: ThemePalette,
        layout: SettingsLayout,
        section: SettingsWindowSection,
        view: SettingsWindowView,
        scroll: i32,
    ) -> io::Result<()> {
        crate::tray::settings_window::render_recovery::recover_once(
            self,
            |renderer| renderer.paint(palette, layout, section, view, scroll),
            || Self::create(window, size.0.max(1), size.1.max(1), dpi),
        )
    }

    pub(super) fn paint(
        &self,
        palette: ThemePalette,
        layout: SettingsLayout,
        section: SettingsWindowSection,
        view: SettingsWindowView,
        scroll: i32,
    ) -> io::Result<()> {
        let panel = brush(&self.target, palette.panel)?;
        let border = brush(&self.target, palette.border)?;
        let accent = brush(&self.target, palette.accent)?;
        let text = brush(&self.target, palette.text_strong)?;
        let mark_text = brush(&self.target, palette.on_accent)?;
        let clear_color = color(palette.canvas);
        // SAFETY: drawing is bracketed by BeginDraw/EndDraw on this UI-thread-owned target.
        unsafe {
            self.target.BeginDraw();
            self.target.Clear(Some(&raw const clear_color));
            self.target.FillRectangle(&rect(layout.header), &panel);
            if layout.sidebar.width > 0 {
                self.target.FillRectangle(&rect(layout.sidebar), &panel);
            }
            if layout.footer.height > 0 {
                self.target.FillRectangle(&rect(layout.footer), &panel);
            }
            draw_separator(
                &self.target,
                layout.header.bottom(),
                layout.header.width,
                &border,
            );
            if layout.footer.height > 0 {
                draw_separator(
                    &self.target,
                    layout.footer.top,
                    layout.footer.width,
                    &border,
                );
            }
            let clip = rect(layout.viewport);
            self.target
                .PushAxisAlignedClip(&raw const clip, D2D1_ANTIALIAS_MODE_ALIASED);
            if matches!(view, SettingsWindowView::ManualDraft) {
                draw_cards(&self.target, layout, section, scroll, &panel, &border);
            } else {
                draw_landing_card(&self.target, layout, view, scroll, &panel, &border);
            }
            self.target.PopAxisAlignedClip();
            let mark = rounded(
                DipRect {
                    left: 24,
                    top: 24,
                    width: 36,
                    height: 36,
                },
                10.0,
            );
            self.target.FillRoundedRectangle(&raw const mark, &accent);
            let symbol = "S".encode_utf16().collect::<Vec<_>>();
            let symbol_rect = D2D_RECT_F {
                left: 34.0,
                top: 31.0,
                right: 54.0,
                bottom: 55.0,
            };
            self.target.DrawText(
                &symbol,
                &self.brand_format,
                &raw const symbol_rect,
                &mark_text,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            let brand = "Sniptale".encode_utf16().collect::<Vec<_>>();
            let brand_rect = D2D_RECT_F {
                left: 72.0,
                top: 26.0,
                right: 180.0,
                bottom: 58.0,
            };
            self.target.DrawText(
                &brand,
                &self.brand_format,
                &raw const brand_rect,
                &text,
                D2D1_DRAW_TEXT_OPTIONS_NONE,
                DWRITE_MEASURING_MODE_NATURAL,
            );
            self.target.EndDraw(None, None)?;
        }
        Ok(())
    }
}

fn draw_landing_card(
    target: &ID2D1HwndRenderTarget,
    layout: SettingsLayout,
    view: SettingsWindowView,
    scroll: i32,
    panel: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    border: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
) {
    let card = rounded(layout.landing_card(view, scroll), 16.0);
    // SAFETY: geometry and brushes remain live for these synchronous target calls.
    unsafe {
        target.FillRoundedRectangle(&raw const card, panel);
        target.DrawRoundedRectangle(&raw const card, border, 1.0, None);
    }
}

fn draw_cards(
    target: &ID2D1HwndRenderTarget,
    layout: SettingsLayout,
    section: SettingsWindowSection,
    scroll: i32,
    panel: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
    border: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
) {
    let top = layout.viewport.top + PAGE_GUTTER - scroll + 64;
    let left = layout.viewport.left + PAGE_GUTTER;
    for (offset, height) in card_geometry(section) {
        let card = rounded(
            DipRect {
                left,
                top: top + offset,
                width: layout.content_width,
                height: *height,
            },
            12.0,
        );
        // SAFETY: geometry and brushes remain live for these synchronous target calls.
        unsafe {
            target.FillRoundedRectangle(&raw const card, panel);
            target.DrawRoundedRectangle(&raw const card, border, 1.0, None);
        }
    }
}

unsafe fn draw_separator(
    target: &ID2D1HwndRenderTarget,
    top: i32,
    width: i32,
    brush: &windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush,
) {
    let rect = D2D_RECT_F {
        left: 0.0,
        top: top as f32,
        right: width as f32,
        bottom: top as f32 + 1.0,
    };
    // SAFETY: called only inside the active draw bracket with a live brush.
    unsafe { target.FillRectangle(&raw const rect, brush) };
}

fn brush(
    target: &ID2D1HwndRenderTarget,
    value: Color,
) -> io::Result<windows::Win32::Graphics::Direct2D::ID2D1SolidColorBrush> {
    let value = color(value);
    // SAFETY: color is initialized and copied by the render target.
    unsafe { target.CreateSolidColorBrush(&raw const value, None) }.map_err(to_io_error)
}
