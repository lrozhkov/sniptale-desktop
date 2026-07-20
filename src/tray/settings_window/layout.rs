//! DPI-independent shell geometry and scroll invariants.

use super::model::{SettingsWindowSection, SettingsWindowView};

pub(crate) const HEADER_HEIGHT: i32 = 92;
pub(crate) const FOOTER_HEIGHT: i32 = 72;
pub(crate) const SIDEBAR_WIDTH: i32 = 196;
pub(crate) const PAGE_GUTTER: i32 = 24;
pub(crate) const MIN_WIDTH: i32 = 760;
pub(crate) const MIN_HEIGHT: i32 = 620;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct DipRect {
    pub(crate) left: i32,
    pub(crate) top: i32,
    pub(crate) width: i32,
    pub(crate) height: i32,
}

impl DipRect {
    #[must_use]
    pub(crate) const fn right(self) -> i32 {
        self.left + self.width
    }

    #[must_use]
    pub(crate) const fn bottom(self) -> i32 {
        self.top + self.height
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SettingsLayout {
    pub(crate) header: DipRect,
    pub(crate) sidebar: DipRect,
    pub(crate) viewport: DipRect,
    pub(crate) footer: DipRect,
    pub(crate) content_width: i32,
    pub(crate) content_height: i32,
    pub(crate) max_scroll: i32,
}

impl SettingsLayout {
    #[must_use]
    pub(crate) fn compute(
        width: i32,
        height: i32,
        section: SettingsWindowSection,
        view: SettingsWindowView,
    ) -> Self {
        let safe_width = width.max(480);
        let safe_height = height.max(360);
        let sidebar_width = if matches!(view, SettingsWindowView::ManualDraft) {
            SIDEBAR_WIDTH
        } else {
            0
        };
        let footer_height = if matches!(view, SettingsWindowView::ExtensionManagedReadOnly) {
            0
        } else {
            FOOTER_HEIGHT
        };
        let footer_top = safe_height - footer_height;
        let viewport_height = (footer_top - HEADER_HEIGHT).max(120);
        let content_height = if matches!(view, SettingsWindowView::ManualDraft) {
            page_height(section)
        } else {
            landing_height(view)
        };
        Self {
            header: DipRect {
                left: 0,
                top: 0,
                width: safe_width,
                height: HEADER_HEIGHT,
            },
            sidebar: DipRect {
                left: 0,
                top: HEADER_HEIGHT,
                width: sidebar_width,
                height: viewport_height,
            },
            viewport: DipRect {
                left: sidebar_width,
                top: HEADER_HEIGHT,
                width: safe_width - sidebar_width,
                height: viewport_height,
            },
            footer: DipRect {
                left: 0,
                top: footer_top,
                width: safe_width,
                height: footer_height,
            },
            content_width: (safe_width - sidebar_width - PAGE_GUTTER * 2).max(280),
            content_height,
            max_scroll: (content_height - viewport_height + PAGE_GUTTER * 2).max(0),
        }
    }

    #[must_use]
    pub(crate) fn landing_card(self, view: SettingsWindowView, scroll: i32) -> DipRect {
        let width = (self.viewport.width - 48).clamp(280, 680);
        let height = landing_height(view);
        let top = if height + PAGE_GUTTER * 2 <= self.viewport.height {
            self.viewport.top + (self.viewport.height - height) / 2
        } else {
            self.viewport.top + PAGE_GUTTER
        };
        DipRect {
            left: self.viewport.left + (self.viewport.width - width) / 2,
            top: top - self.clamp_scroll(scroll),
            width,
            height,
        }
    }

    #[must_use]
    pub(crate) const fn clamp_scroll(self, value: i32) -> i32 {
        if value < 0 {
            0
        } else if value > self.max_scroll {
            self.max_scroll
        } else {
            value
        }
    }
}

const fn page_height(section: SettingsWindowSection) -> i32 {
    match section {
        SettingsWindowSection::Capture => 380,
        SettingsWindowSection::Video => 500,
        SettingsWindowSection::Audio => 400,
        SettingsWindowSection::Shortcuts => 560,
    }
}

pub(crate) const fn card_geometry(section: SettingsWindowSection) -> &'static [(i32, i32)] {
    match section {
        SettingsWindowSection::Capture => &[(0, 150), (168, 112)],
        SettingsWindowSection::Video => &[(0, 390)],
        SettingsWindowSection::Audio => &[(0, 270)],
        SettingsWindowSection::Shortcuts => &[(0, 480)],
    }
}

const fn landing_height(view: SettingsWindowView) -> i32 {
    match view {
        SettingsWindowView::ExtensionManagedReadOnly => 296,
        SettingsWindowView::ExtensionManagedDraft => 232,
        SettingsWindowView::ManualDraft => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_never_overlaps_header_or_viewport() {
        for (width, height) in [(760, 620), (960, 720), (480, 360)] {
            for section in SettingsWindowSection::ALL {
                let layout = SettingsLayout::compute(
                    width,
                    height,
                    section,
                    SettingsWindowView::ManualDraft,
                );
                assert_eq!(layout.header.bottom(), layout.viewport.top);
                assert_eq!(layout.viewport.bottom(), layout.footer.top);
                assert_eq!(layout.footer.bottom(), height.max(360));
                assert_eq!(layout.sidebar.right(), layout.viewport.left);
            }
        }
    }

    #[test]
    fn only_content_scrolls_and_offset_is_bounded() {
        assert_eq!((MIN_WIDTH, MIN_HEIGHT), (760, 620));
        let layout = SettingsLayout::compute(
            760,
            620,
            SettingsWindowSection::Shortcuts,
            SettingsWindowView::ManualDraft,
        );
        assert!(layout.max_scroll > 0);
        assert_eq!(layout.clamp_scroll(-20), 0);
        assert_eq!(layout.clamp_scroll(i32::MAX), layout.max_scroll);
        assert_eq!(layout.footer.top, 548);
    }

    #[test]
    fn read_only_landing_uses_the_full_body_and_has_no_empty_footer_or_sidebar() {
        let layout = SettingsLayout::compute(
            960,
            720,
            SettingsWindowSection::Capture,
            SettingsWindowView::ExtensionManagedReadOnly,
        );
        assert_eq!(layout.sidebar.width, 0);
        assert_eq!(layout.footer.height, 0);
        assert_eq!(layout.viewport.width, 960);
        assert_eq!(layout.viewport.bottom(), 720);
        let card = layout.landing_card(SettingsWindowView::ExtensionManagedReadOnly, 0);
        assert!(card.left > 0);
        assert!(card.right() < layout.viewport.right());
        assert!(card.bottom() <= layout.viewport.bottom());
    }

    #[test]
    fn extension_mode_switch_keeps_actions_visible_without_an_empty_sidebar() {
        let layout = SettingsLayout::compute(
            960,
            720,
            SettingsWindowSection::Capture,
            SettingsWindowView::ExtensionManagedDraft,
        );
        assert_eq!(layout.sidebar.width, 0);
        assert_eq!(layout.footer.height, FOOTER_HEIGHT);
        assert_eq!(layout.viewport.bottom(), layout.footer.top);
    }

    #[test]
    fn small_landing_views_scroll_the_last_content_into_the_viewport() {
        for view in [
            SettingsWindowView::ExtensionManagedReadOnly,
            SettingsWindowView::ExtensionManagedDraft,
        ] {
            let layout = SettingsLayout::compute(480, 360, SettingsWindowSection::Capture, view);
            assert!(layout.max_scroll > 0, "{view:?}");
            assert!(layout.landing_card(view, 0).bottom() > layout.viewport.bottom());
            assert_eq!(
                layout.landing_card(view, layout.max_scroll).bottom(),
                layout.viewport.bottom() - PAGE_GUTTER,
                "{view:?}"
            );
        }
    }

    #[test]
    fn nominal_window_does_not_scroll_non_shortcut_pages() {
        for section in [
            SettingsWindowSection::Capture,
            SettingsWindowSection::Video,
            SettingsWindowSection::Audio,
        ] {
            let layout =
                SettingsLayout::compute(960, 720, section, SettingsWindowView::ManualDraft);
            assert_eq!(layout.max_scroll, 0, "{section:?}");
        }
    }

    #[test]
    fn every_manual_section_has_nonempty_card_geometry() {
        for section in SettingsWindowSection::ALL {
            let cards = card_geometry(section);
            assert!(!cards.is_empty());
            assert!(cards.iter().all(|(_, height)| *height > 0));
        }
    }
}
