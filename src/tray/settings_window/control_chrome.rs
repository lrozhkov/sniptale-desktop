//! Deterministic visual-state projection for owner-drawn native buttons.

use super::palette::{Color, ThemePalette};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ButtonVisual {
    Primary,
    Secondary,
    Navigation { selected: bool },
    Hotkey { active: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ControlTextAlignment {
    Left,
    Center,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ButtonChrome {
    pub(crate) background: Color,
    pub(crate) border: Color,
    pub(crate) foreground: Color,
    pub(crate) alignment: ControlTextAlignment,
}

pub(crate) fn button_chrome(
    palette: ThemePalette,
    visual: ButtonVisual,
    disabled: bool,
    active: bool,
) -> ButtonChrome {
    let opaque = palette.opaque_controls();
    let (background, border, foreground, alignment) = match visual {
        ButtonVisual::Primary if disabled => (
            opaque.accent_soft,
            opaque.accent_soft,
            palette.text_muted,
            ControlTextAlignment::Center,
        ),
        ButtonVisual::Primary => (
            palette.accent,
            palette.accent,
            palette.on_accent,
            ControlTextAlignment::Center,
        ),
        ButtonVisual::Navigation { selected: true } => (
            opaque.accent_soft,
            opaque.accent_soft,
            palette.accent,
            ControlTextAlignment::Left,
        ),
        ButtonVisual::Navigation { selected: false } => (
            if active { opaque.hover } else { opaque.panel },
            opaque.panel,
            if disabled {
                palette.text_muted
            } else {
                palette.text
            },
            ControlTextAlignment::Left,
        ),
        ButtonVisual::Hotkey { active: true } => (
            opaque.accent_soft,
            palette.accent,
            palette.accent,
            ControlTextAlignment::Left,
        ),
        ButtonVisual::Hotkey { active: false } | ButtonVisual::Secondary => (
            if active { opaque.hover } else { opaque.input },
            opaque.border_strong,
            if disabled {
                palette.text_muted
            } else {
                palette.text
            },
            if matches!(visual, ButtonVisual::Hotkey { .. }) {
                ControlTextAlignment::Left
            } else {
                ControlTextAlignment::Center
            },
        ),
    };
    ButtonChrome {
        background,
        border,
        foreground,
        alignment,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn primary_disabled_and_navigation_selected_have_distinct_accessible_chrome() {
        for palette in [ThemePalette::light(), ThemePalette::dark()] {
            let primary = button_chrome(palette, ButtonVisual::Primary, true, false);
            let navigation = button_chrome(
                palette,
                ButtonVisual::Navigation { selected: true },
                false,
                false,
            );
            assert_eq!(primary.background.alpha, 255);
            assert_eq!(navigation.background.alpha, 255);
            assert_eq!(navigation.foreground, palette.accent);
            assert_ne!(primary.foreground, primary.background);
            assert_ne!(navigation.foreground, navigation.background);
            for visual in [
                ButtonVisual::Secondary,
                ButtonVisual::Hotkey { active: false },
            ] {
                let chrome = button_chrome(palette, visual, false, false);
                assert_eq!(chrome.background.alpha, 255);
            }
        }
    }

    #[test]
    fn high_contrast_primary_uses_system_highlight_text() {
        let colors = crate::tray::settings_window::palette::SystemContrastColors {
            canvas: Color {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 255,
            },
            panel: Color {
                red: 16,
                green: 16,
                blue: 16,
                alpha: 255,
            },
            text: Color {
                red: 255,
                green: 255,
                blue: 255,
                alpha: 255,
            },
            muted_text: Color {
                red: 192,
                green: 192,
                blue: 192,
                alpha: 255,
            },
            accent: Color {
                red: 255,
                green: 255,
                blue: 255,
                alpha: 255,
            },
            accent_text: Color {
                red: 0,
                green: 0,
                blue: 0,
                alpha: 255,
            },
        };
        let palette = ThemePalette::high_contrast(colors);
        let chrome = button_chrome(palette, ButtonVisual::Primary, false, false);
        assert_eq!(chrome.background, colors.accent);
        assert_eq!(chrome.foreground, colors.accent_text);
    }
}
