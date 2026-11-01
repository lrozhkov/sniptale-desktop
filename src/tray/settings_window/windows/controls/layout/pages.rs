//! Per-section placement inside the shared scrollable viewport.

use super::{SettingsLayout, SettingsNativeWindow, layout_page_heading, page_origin, place_page};

pub(super) fn capture(window: &SettingsNativeWindow, layout: SettingsLayout) {
    let page = &window.pages.capture;
    layout_page_heading(window, layout, page.title, page.subtitle);
    let (left, top) = page_origin(window, layout);
    let width = layout.content_width;
    place_page(
        window,
        layout,
        page.save_label,
        (left + 20, top + 84, 200, 24),
    );
    place_page(
        window,
        layout,
        page.destination_path,
        (left + 20, top + 116, (width - 224).max(160), 28),
    );
    place_page(
        window,
        layout,
        page.browse,
        (left + width - 172, top + 106, 152, 38),
    );
    place_page(
        window,
        layout,
        page.screenshot_label,
        (left + 20, top + 490, 200, 24),
    );
    place_page(
        window,
        layout,
        page.screenshot_cursor,
        (left + 20, top + 522, 300, 30),
    );
    delivery_controls(window, layout);
}

pub(super) fn video(window: &SettingsNativeWindow, layout: SettingsLayout) {
    let page = &window.pages.video;
    layout_page_heading(window, layout, page.title, page.subtitle);
    let (left, top) = page_origin(window, layout);
    let gap = 16;
    let column = (layout.content_width - 56) / 2;
    let controls = [
        page.quality,
        page.fps,
        page.bitrate,
        page.hardware,
        page.duration,
    ];
    for (index, (label, control)) in page.labels.iter().zip(controls).enumerate() {
        let row = i32::try_from(index / 2).unwrap_or(0);
        let column_index = i32::try_from(index % 2).unwrap_or(0);
        let x = left + 20 + column_index * (column + gap);
        let y = top + 88 + row * 82;
        place_page(window, layout, *label, (x, y, column, 22));
        place_page(window, layout, control, (x, y + 24, column, 240));
    }
    place_page(window, layout, page.cursor, (left + 20, top + 342, 280, 30));
    place_page(
        window,
        layout,
        page.pointer_telemetry,
        (left + 20, top + 384, 360, 30),
    );
}

pub(super) fn audio(window: &SettingsNativeWindow, layout: SettingsLayout) {
    let page = &window.pages.audio;
    layout_page_heading(window, layout, page.title, page.subtitle);
    let (left, top) = page_origin(window, layout);
    let field_left = left + 20;
    let width = layout.content_width - 40;
    place_page(
        window,
        layout,
        page.labels[0],
        (field_left, top + 88, 180, 22),
    );
    place_page(
        window,
        layout,
        page.source,
        (field_left, top + 112, 220, 240),
    );
    place_page(
        window,
        layout,
        page.labels[1],
        (field_left, top + 176, 200, 22),
    );
    place_page(
        window,
        layout,
        page.microphone,
        (field_left, top + 200, (width - 154).max(220), 260),
    );
    place_page(
        window,
        layout,
        page.refresh,
        (left + layout.content_width - 134, top + 198, 114, 38),
    );
    place_page(
        window,
        layout,
        page.labels[2],
        (field_left, top + 264, 180, 22),
    );
    place_page(
        window,
        layout,
        page.bitrate,
        (field_left, top + 288, 220, 240),
    );
}

pub(super) fn shortcuts(window: &SettingsNativeWindow, layout: SettingsLayout) {
    let page = &window.pages.shortcuts;
    layout_page_heading(window, layout, page.title, page.subtitle);
    let (left, top) = page_origin(window, layout);
    let field_width = 210.min((layout.content_width - 240).max(150));
    for (index, (_, label, field)) in page.hotkeys.iter().enumerate() {
        let y = top + 84 + i32::try_from(index).unwrap_or(0) * 52;
        let label_width = (layout.content_width - field_width - 64).max(160);
        place_page(window, layout, *label, (left + 20, y + 8, label_width, 24));
        place_page(
            window,
            layout,
            *field,
            (
                left + layout.content_width - field_width - 20,
                y,
                field_width,
                38,
            ),
        );
    }
}

fn delivery_controls(window: &SettingsNativeWindow, layout: SettingsLayout) {
    let page = &window.pages.capture;
    let (left, top) = page_origin(window, layout);
    let width = layout.content_width;
    place_page(
        window,
        layout,
        page.destination_label,
        (left + 20, top + 248, width - 40, 24),
    );
    place_page(
        window,
        layout,
        page.browser_destination,
        (left + 20, top + 280, width - 40, 240),
    );
    place_page(
        window,
        layout,
        page.ask_destination,
        (left + 20, top + 324, width - 40, 30),
    );
    place_page(
        window,
        layout,
        page.save_destination,
        (left + 20, top + 372, 180, 38),
    );
    place_page(
        window,
        layout,
        page.refresh_targets,
        (left + 216, top + 372, 160, 38),
    );
}
