//! Responsive placement of the native settings control tree.

use super::*;
use crate::tray::settings_window::layout::{DipRect, PAGE_GUTTER, SettingsLayout};

#[path = "layout/pages.rs"]
mod pages;

pub(super) fn apply(window: &SettingsNativeWindow) {
    let layout = window.current_layout();
    layout_header(window, layout);
    if window.presentation.shows_navigation() {
        layout_navigation(window, layout);
    }
    if window.presentation.shows_footer() {
        layout_footer(window, layout);
    }
    if window.presentation.manual().is_some() {
        layout_page(window, layout);
    } else {
        layout_landing(window, layout);
    }
    update_scrollbar(window.handle, layout, window.scroll);
}

fn layout_header(window: &SettingsNativeWindow, layout: SettingsLayout) {
    let width = layout.header.width;
    place(
        window.shell.headline,
        (196, 20, (width - 420).max(180), 32),
        window.dpi,
    );
    place(
        window.shell.body,
        (196, 54, (width - 420).max(180), 24),
        window.dpi,
    );
    place(
        window.shell.theme_label,
        (width - 264, 36, 64, 24),
        window.dpi,
    );
    place(window.shell.theme, (width - 190, 28, 156, 240), window.dpi);
}

fn layout_navigation(window: &SettingsNativeWindow, layout: SettingsLayout) {
    for (index, (_, handle)) in window.shell.navigation.iter().enumerate() {
        let top = layout.sidebar.top + 20 + i32::try_from(index).unwrap_or(0) * 46;
        place(*handle, (16, top, 164, 38), window.dpi);
    }
    place(
        window.shell.extension_mode,
        (16, layout.sidebar.bottom() - 54, 164, 38),
        window.dpi,
    );
}

fn layout_footer(window: &SettingsNativeWindow, layout: SettingsLayout) {
    let right = layout.footer.right() - 24;
    let top = layout.footer.top + 18;
    place(window.shell.apply, (right - 190, top, 190, 38), window.dpi);
    place(window.shell.cancel, (right - 304, top, 102, 38), window.dpi);
    layout_footer_messages(window, layout);
    place(
        window.shell.keep_editing,
        (right - 232, top, 126, 38),
        window.dpi,
    );
    place(
        window.shell.confirm_discard,
        (right - 94, top, 94, 38),
        window.dpi,
    );
}

fn layout_footer_messages(window: &SettingsNativeWindow, layout: SettingsLayout) {
    let left = layout.viewport.left + PAGE_GUTTER;
    let top = layout.footer.top + 24;
    place(
        window.shell.feedback,
        (left, top, (layout.footer.width - 560).max(180), 24),
        window.dpi,
    );
    place(window.shell.confirm_text, (left, top, 260, 24), window.dpi);
}

fn layout_landing(window: &SettingsNativeWindow, layout: SettingsLayout) {
    let card = layout.landing_card(window.presentation.view(), window.scroll);
    place_page(
        window,
        layout,
        window.shell.landing_badge,
        (card.left + 32, card.top + 32, card.width - 64, 22),
    );
    place_page(
        window,
        layout,
        window.shell.landing_title,
        (card.left + 32, card.top + 66, card.width - 64, 34),
    );
    place_page(
        window,
        layout,
        window.shell.landing_body,
        (card.left + 32, card.top + 110, card.width - 64, 58),
    );
    place_page(
        window,
        layout,
        window.pages.capture.browse,
        (card.left + 32, card.top + 182, 196, 40),
    );
    place_page(
        window,
        layout,
        window.shell.landing_note,
        (card.left + 32, card.bottom() - 34, card.width - 64, 22),
    );
    if !window.presentation.shows_footer() {
        place_page(
            window,
            layout,
            window.shell.feedback,
            (card.left + 32, card.bottom() - 62, card.width - 64, 22),
        );
    }
}

fn layout_page(window: &SettingsNativeWindow, layout: SettingsLayout) {
    match window.presentation.section() {
        SettingsWindowSection::Capture => pages::capture(window, layout),
        SettingsWindowSection::Video => pages::video(window, layout),
        SettingsWindowSection::Audio => pages::audio(window, layout),
        SettingsWindowSection::Shortcuts => pages::shortcuts(window, layout),
    }
}

fn page_origin(window: &SettingsNativeWindow, layout: SettingsLayout) -> (i32, i32) {
    (
        layout.viewport.left + PAGE_GUTTER,
        layout.viewport.top + PAGE_GUTTER - window.scroll,
    )
}

fn layout_page_heading(
    window: &SettingsNativeWindow,
    layout: SettingsLayout,
    title: HWND,
    subtitle: HWND,
) {
    let (left, top) = page_origin(window, layout);
    place_page(window, layout, title, (left, top, layout.content_width, 32));
    place_page(
        window,
        layout,
        subtitle,
        (left, top + 34, layout.content_width, 24),
    );
}

fn place_page(
    window: &SettingsNativeWindow,
    layout: SettingsLayout,
    handle: HWND,
    bounds: (i32, i32, i32, i32),
) {
    move_page(window, layout, handle, dip(bounds));
}

fn move_page(window: &SettingsNativeWindow, layout: SettingsLayout, handle: HWND, rect: DipRect) {
    let visible_bottom = rect.top + rect.height.min(48);
    let visible = rect.top >= layout.viewport.top && visible_bottom <= layout.viewport.bottom();
    native::set_visible(handle, visible);
    move_dip(handle, rect, window.dpi);
}

fn place(handle: HWND, bounds: (i32, i32, i32, i32), dpi: u32) {
    move_dip(handle, dip(bounds), dpi);
}

const fn dip(bounds: (i32, i32, i32, i32)) -> DipRect {
    DipRect {
        left: bounds.0,
        top: bounds.1,
        width: bounds.2,
        height: bounds.3,
    }
}
