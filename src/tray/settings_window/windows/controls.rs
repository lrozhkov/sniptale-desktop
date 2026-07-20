//! Native child-control ownership and adaptive presentation.
#![allow(unsafe_code)]

use super::*;
use std::io;
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Gdi::{CreateSolidBrush, DeleteObject, HBRUSH, HDC},
    UI::{
        Controls::DRAWITEMSTRUCT,
        HiDpi::GetDpiForWindow,
        WindowsAndMessaging::{CreateWindowExW, WINDOW_EX_STYLE},
    },
};
use windows::core::PCWSTR;

use crate::tray::settings_window::{
    control_chrome::ButtonVisual,
    hotkey_capture::HotkeyCaptureState,
    layout::SettingsLayout,
    model::{SettingsWindowPresentation, SettingsWindowSection},
    palette::{EffectiveTheme, ThemePalette},
};

#[path = "controls/input.rs"]
mod input;
#[path = "controls/layout.rs"]
mod layout;
#[path = "controls/native.rs"]
mod native;
#[path = "controls/presentation.rs"]
mod presentation;
#[path = "controls/tooltip.rs"]
mod tooltip;
#[path = "controls/tree.rs"]
mod tree;

use native::{
    NativeFonts, apply_control_theme, checkbox_color, client_size, create_control, invalidate,
    move_dip, static_color, update_scrollbar,
};

#[derive(Debug)]
struct ControlBackgroundBrush(HBRUSH);

impl ControlBackgroundBrush {
    fn create(color: crate::tray::settings_window::palette::Color) -> Self {
        // SAFETY: CreateSolidBrush copies the color and returns an independently owned GDI brush.
        Self(unsafe { CreateSolidBrush(native::color_ref(color)) })
    }

    fn as_lresult(&self) -> LRESULT {
        LRESULT(self.0.0 as isize)
    }
}

impl Drop for ControlBackgroundBrush {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: this RAII wrapper owns an unselected brush and deletes it exactly once.
            let _ = unsafe { DeleteObject(self.0.into()) };
            self.0 = HBRUSH::default();
        }
    }
}

#[derive(Debug)]
pub(super) struct SettingsNativeWindow {
    pub(super) handle: HWND,
    shell: tree::ShellControls,
    pages: tree::ManualControls,
    renderer: super::render::SettingsRenderer,
    fonts: NativeFonts,
    dpi: u32,
    scroll: i32,
    presentation: SettingsWindowPresentation,
    palette: ThemePalette,
    effective_theme: EffectiveTheme,
    control_background: ControlBackgroundBrush,
    capture: HotkeyCaptureState,
    destination_tooltip: tooltip::NativeTooltip,
}

impl SettingsNativeWindow {
    pub(super) fn create(
        handle: HWND,
        presentation: SettingsWindowPresentation,
    ) -> io::Result<Self> {
        // SAFETY: querying DPI retains no pointers and the HWND is live on this UI thread.
        let dpi = unsafe { GetDpiForWindow(handle) }.max(96);
        let (width, height) = client_size(handle);
        let shell = tree::create_shell(handle)?;
        let pages = tree::create_manual(handle)?;
        let destination_tooltip = tooltip::NativeTooltip::create(
            handle,
            pages.capture.destination_path,
            "Choose a folder",
        )?;
        let renderer = super::render::SettingsRenderer::create(handle, width, height, dpi)?;
        let fonts = NativeFonts::create(dpi);
        let (effective_theme, palette) = super::theme::resolve_theme(presentation.theme());
        let control_background = ControlBackgroundBrush::create(palette.opaque_controls().panel);
        let mut window = Self {
            handle,
            shell,
            pages,
            renderer,
            fonts,
            dpi,
            scroll: 0,
            presentation,
            palette,
            effective_theme,
            control_background,
            capture: HotkeyCaptureState::default(),
            destination_tooltip,
        };
        window.apply_fonts();
        window.apply_theme();
        presentation::refresh(&mut window)?;
        Ok(window)
    }

    pub(super) fn refresh(&mut self, presentation: SettingsWindowPresentation) -> io::Result<()> {
        self.capture.cancel();
        if self.presentation.section() != presentation.section() {
            self.scroll = 0;
        }
        self.presentation = presentation;
        if let Some(manual) = self.presentation.manual() {
            self.destination_tooltip.update(&manual.destination_path);
        }
        let (effective, palette) = super::theme::resolve_theme(self.presentation.theme());
        if effective != self.effective_theme || palette != self.palette {
            self.effective_theme = effective;
            self.palette = palette;
            self.control_background =
                ControlBackgroundBrush::create(self.palette.opaque_controls().panel);
            self.apply_theme();
        }
        presentation::refresh(self)
    }

    pub(super) fn resize(&mut self, width: u32, height: u32) {
        // SAFETY: querying the DPI of this live adapter-owned HWND retains no pointers.
        let dpi = unsafe { GetDpiForWindow(self.handle) }.max(96);
        if dpi != self.dpi {
            self.dpi = dpi;
            self.fonts = NativeFonts::create(dpi);
            self.apply_fonts();
        }
        let _ = self
            .renderer
            .resize_recovering(self.handle, width.max(1), height.max(1), dpi);
        self.scroll = self.current_layout().clamp_scroll(self.scroll);
        layout::apply(self);
        invalidate(self.handle);
    }

    pub(super) fn scroll_by(&mut self, delta: i32) {
        let layout = self.current_layout();
        let next = layout.clamp_scroll(self.scroll.saturating_add(delta));
        if next != self.scroll {
            self.scroll = next;
            layout::apply(self);
            invalidate(self.handle);
        }
    }

    pub(super) fn scroll_to(&mut self, position: i32) {
        self.scroll_by(position.saturating_sub(self.scroll));
    }

    pub(super) fn refresh_system_theme(&mut self) {
        let presentation = self.presentation.clone();
        let _ = self.refresh(presentation);
    }

    pub(super) fn paint(&mut self) -> io::Result<()> {
        self.renderer.paint_recovering(
            self.handle,
            client_size(self.handle),
            self.dpi,
            self.palette,
            self.current_layout(),
            self.presentation.section(),
            self.presentation.view(),
            self.scroll,
        )
    }

    pub(super) fn static_color(&self, control: HWND, hdc: HDC) -> LRESULT {
        let color = if control == self.shell.feedback {
            self.palette.danger
        } else if control == self.shell.landing_badge {
            self.palette.accent
        } else if self.muted_handles().contains(&control) {
            self.palette.text_muted
        } else {
            self.palette.text
        };
        if control == self.pages.capture.screenshot_cursor || control == self.pages.video.cursor {
            checkbox_color(
                hdc,
                color,
                self.palette.opaque_controls().panel,
                &self.control_background,
            )
        } else {
            static_color(hdc, color)
        }
    }

    pub(super) fn draw_item(&self, item: &DRAWITEMSTRUCT) {
        let id = item.CtlID as usize;
        let selected = self.nav_section(id) == Some(self.presentation.section())
            || id
                .checked_sub(HOTKEY_FIRST_ID)
                .and_then(|index| self.pages.shortcuts.hotkeys.get(index))
                .is_some_and(|(action, _, _)| self.capture.active_action() == Some(*action));
        let visual = if id == APPLY_ID {
            ButtonVisual::Primary
        } else if self.nav_section(id).is_some() {
            ButtonVisual::Navigation { selected }
        } else if (HOTKEY_FIRST_ID..HOTKEY_FIRST_ID + 8).contains(&id) {
            ButtonVisual::Hotkey { active: selected }
        } else {
            ButtonVisual::Secondary
        };
        super::render::draw_control(item, self.palette, visual);
    }

    pub(super) fn current_layout(&self) -> SettingsLayout {
        let (width, height) = client_size(self.handle);
        let unscale = |value: u32| i32::try_from(value.saturating_mul(96) / self.dpi).unwrap_or(0);
        SettingsLayout::compute(
            unscale(width),
            unscale(height),
            self.presentation.section(),
            self.presentation.view(),
        )
    }

    fn nav_section(&self, id: usize) -> Option<SettingsWindowSection> {
        id.checked_sub(NAV_FIRST_ID)
            .and_then(|index| SettingsWindowSection::ALL.get(index).copied())
    }

    fn apply_fonts(&self) {
        let mut all = self.shell.handles();
        all.extend(self.pages.all_handles());
        let titles = [
            self.shell.headline,
            self.shell.landing_title,
            self.pages.capture.title,
            self.pages.video.title,
            self.pages.audio.title,
            self.pages.shortcuts.title,
        ];
        self.fonts.apply(&all, &titles);
        native::set_combo_metrics(
            &[
                self.shell.theme,
                self.pages.video.quality,
                self.pages.video.fps,
                self.pages.video.bitrate,
                self.pages.video.hardware,
                self.pages.video.duration,
                self.pages.audio.source,
                self.pages.audio.microphone,
                self.pages.audio.bitrate,
            ],
            self.dpi,
        );
    }

    fn apply_theme(&self) {
        super::theme::apply_title_bar(self.handle, self.effective_theme);
        let mut interactive = self.shell.interactive_handles();
        interactive.extend(self.pages.interactive_handles());
        apply_control_theme(&interactive, self.effective_theme == EffectiveTheme::Dark);
        invalidate(self.handle);
    }

    fn muted_handles(&self) -> [HWND; 8] {
        [
            self.shell.body,
            self.shell.landing_body,
            self.shell.landing_note,
            self.pages.capture.subtitle,
            self.pages.capture.destination_path,
            self.pages.video.subtitle,
            self.pages.audio.subtitle,
            self.pages.shortcuts.subtitle,
        ]
    }
}
