//! Sniptale light/dark tokens and deterministic high-contrast projection.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Color {
    pub(crate) red: u8,
    pub(crate) green: u8,
    pub(crate) blue: u8,
    pub(crate) alpha: u8,
}

impl Color {
    const fn rgb(red: u8, green: u8, blue: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha: 255,
        }
    }

    const fn rgba(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {
            red,
            green,
            blue,
            alpha,
        }
    }

    #[must_use]
    pub(crate) const fn over(self, background: Self) -> Self {
        let alpha = self.alpha as u32;
        let inverse = 255 - alpha;
        Self::rgb(
            blend(self.red, background.red, alpha, inverse),
            blend(self.green, background.green, alpha, inverse),
            blend(self.blue, background.blue, alpha, inverse),
        )
    }
}

const fn blend(foreground: u8, background: u8, alpha: u32, inverse: u32) -> u8 {
    let value = (foreground as u32 * alpha + background as u32 * inverse + 127) / 255;
    value.to_le_bytes()[0]
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EffectiveTheme {
    Light,
    Dark,
    HighContrast,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SystemContrastColors {
    pub(crate) canvas: Color,
    pub(crate) panel: Color,
    pub(crate) text: Color,
    pub(crate) muted_text: Color,
    pub(crate) accent: Color,
    pub(crate) accent_text: Color,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ThemePalette {
    pub(crate) canvas: Color,
    pub(crate) panel: Color,
    pub(crate) input: Color,
    pub(crate) hover: Color,
    pub(crate) border: Color,
    pub(crate) border_strong: Color,
    pub(crate) text: Color,
    pub(crate) text_strong: Color,
    pub(crate) text_muted: Color,
    pub(crate) accent: Color,
    pub(crate) on_accent: Color,
    pub(crate) accent_soft: Color,
    pub(crate) danger: Color,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OpaqueControlPalette {
    pub(crate) panel: Color,
    pub(crate) input: Color,
    pub(crate) hover: Color,
    pub(crate) border: Color,
    pub(crate) border_strong: Color,
    pub(crate) accent_soft: Color,
}

impl ThemePalette {
    #[must_use]
    pub(crate) const fn light() -> Self {
        Self {
            canvas: Color::rgb(0xf6, 0xf2, 0xed),
            panel: Color::rgba(0xff, 0xfc, 0xf8, 250),
            input: Color::rgba(0x27, 0x27, 0x2a, 14),
            hover: Color::rgba(0x27, 0x27, 0x2a, 19),
            border: Color::rgba(0x43, 0x28, 0x18, 28),
            border_strong: Color::rgba(0x27, 0x27, 0x2a, 46),
            text: Color::rgb(0x27, 0x27, 0x2a),
            text_strong: Color::rgb(0x09, 0x09, 0x0b),
            text_muted: Color::rgb(0x6b, 0x72, 0x80),
            accent: Color::rgb(0xea, 0x58, 0x0c),
            on_accent: Color::rgb(0xff, 0xff, 0xff),
            accent_soft: Color::rgba(0xea, 0x58, 0x0c, 31),
            danger: Color::rgb(0xe1, 0x1d, 0x48),
        }
    }

    #[must_use]
    pub(crate) const fn dark() -> Self {
        Self {
            canvas: Color::rgb(0x09, 0x09, 0x0b),
            panel: Color::rgba(0x18, 0x18, 0x1b, 242),
            input: Color::rgba(0xff, 0xff, 0xff, 13),
            hover: Color::rgba(0xff, 0xff, 0xff, 20),
            border: Color::rgba(0xff, 0xff, 0xff, 26),
            border_strong: Color::rgba(0xff, 0xff, 0xff, 38),
            text: Color::rgb(0xf4, 0xf4, 0xf5),
            text_strong: Color::rgb(0xfa, 0xfa, 0xfa),
            text_muted: Color::rgb(0x8d, 0x8d, 0x96),
            accent: Color::rgb(0xf9, 0x73, 0x16),
            on_accent: Color::rgb(0xff, 0xff, 0xff),
            accent_soft: Color::rgba(0xf9, 0x73, 0x16, 38),
            danger: Color::rgb(0xef, 0x44, 0x44),
        }
    }

    #[must_use]
    pub(crate) const fn high_contrast(system: SystemContrastColors) -> Self {
        Self {
            canvas: system.canvas,
            panel: system.panel,
            input: system.panel,
            hover: system.panel,
            border: system.text,
            border_strong: system.text,
            text: system.text,
            text_strong: system.text,
            text_muted: system.muted_text,
            accent: system.accent,
            on_accent: system.accent_text,
            accent_soft: system.panel,
            danger: system.accent,
        }
    }

    #[must_use]
    pub(crate) const fn opaque_controls(self) -> OpaqueControlPalette {
        let panel = self.panel.over(self.canvas);
        OpaqueControlPalette {
            panel,
            input: self.input.over(panel),
            hover: self.hover.over(panel),
            border: self.border.over(panel),
            border_strong: self.border_strong.over(panel),
            accent_soft: self.accent_soft.over(panel),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branded_palettes_match_sniptale_surface_tokens() {
        assert_ne!(EffectiveTheme::Light, EffectiveTheme::Dark);
        assert_ne!(EffectiveTheme::HighContrast, EffectiveTheme::Dark);
        assert_eq!(ThemePalette::light().canvas, Color::rgb(0xf6, 0xf2, 0xed));
        assert_eq!(ThemePalette::light().accent, Color::rgb(0xea, 0x58, 0x0c));
        assert_eq!(ThemePalette::dark().canvas, Color::rgb(0x09, 0x09, 0x0b));
        assert_eq!(ThemePalette::dark().accent, Color::rgb(0xf9, 0x73, 0x16));
    }

    #[test]
    fn high_contrast_uses_only_system_colors() {
        let colors = SystemContrastColors {
            canvas: Color::rgb(1, 2, 3),
            panel: Color::rgb(4, 5, 6),
            text: Color::rgb(7, 8, 9),
            muted_text: Color::rgb(10, 11, 12),
            accent: Color::rgb(13, 14, 15),
            accent_text: Color::rgb(16, 17, 18),
        };
        let palette = ThemePalette::high_contrast(colors);
        assert_eq!(palette.canvas, colors.canvas);
        assert_eq!(palette.text, colors.text);
        assert_eq!(palette.accent, colors.accent);
        assert_eq!(palette.on_accent, colors.accent_text);
    }

    #[test]
    fn translucent_tokens_are_flattened_before_gdi_control_rendering() {
        let light = ThemePalette::light().opaque_controls();
        let dark = ThemePalette::dark().opaque_controls();
        for color in [
            light.panel,
            light.input,
            light.hover,
            light.border,
            light.border_strong,
            light.accent_soft,
            dark.panel,
            dark.input,
            dark.hover,
            dark.border,
            dark.border_strong,
            dark.accent_soft,
        ] {
            assert_eq!(color.alpha, 255);
        }
        assert_ne!(light.input, ThemePalette::light().text);
        assert_ne!(dark.input, ThemePalette::dark().text);
        assert!(light.input.red > 0xc0);
        assert!(dark.input.red < 0x40);
    }
}
