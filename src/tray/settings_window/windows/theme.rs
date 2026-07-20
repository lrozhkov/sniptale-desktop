//! Win32 system-theme, high-contrast, and title-bar adapter.
#![allow(unsafe_code)]

use std::mem::size_of;

use windows::{
    Win32::{
        Foundation::{ERROR_SUCCESS, HWND},
        Graphics::{
            Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute},
            Gdi::{
                COLOR_BTNFACE, COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_WINDOW,
                COLOR_WINDOWTEXT, GetSysColor,
            },
        },
        System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_READ, REG_DWORD, REG_VALUE_TYPE, RegCloseKey,
            RegOpenKeyExW, RegQueryValueExW,
        },
        UI::{
            Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
            WindowsAndMessaging::{
                SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
            },
        },
    },
    core::PCWSTR,
};

use crate::{
    settings::AppearanceThemePreference,
    tray::settings_window::palette::{Color, EffectiveTheme, SystemContrastColors, ThemePalette},
};

const PERSONALIZE_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";
const APPS_USE_LIGHT_THEME: &str = "AppsUseLightTheme";

pub(super) fn resolve_theme(
    preference: AppearanceThemePreference,
) -> (EffectiveTheme, ThemePalette) {
    if high_contrast_enabled() {
        return (
            EffectiveTheme::HighContrast,
            ThemePalette::high_contrast(system_contrast_colors()),
        );
    }
    let effective = match preference {
        AppearanceThemePreference::Light => EffectiveTheme::Light,
        AppearanceThemePreference::Dark => EffectiveTheme::Dark,
        AppearanceThemePreference::System => {
            if system_prefers_dark() {
                EffectiveTheme::Dark
            } else {
                EffectiveTheme::Light
            }
        }
    };
    let palette = match effective {
        EffectiveTheme::Dark => ThemePalette::dark(),
        EffectiveTheme::Light => ThemePalette::light(),
        EffectiveTheme::HighContrast => ThemePalette::high_contrast(system_contrast_colors()),
    };
    (effective, palette)
}

pub(super) fn apply_title_bar(window: HWND, theme: EffectiveTheme) {
    let enabled = u32::from(theme == EffectiveTheme::Dark);
    // SAFETY: DWM reads a correctly sized u32 synchronously; the HWND remains adapter-owned.
    let _ = unsafe {
        DwmSetWindowAttribute(
            window,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&raw const enabled).cast(),
            size_of::<u32>() as u32,
        )
    };
}

fn high_contrast_enabled() -> bool {
    let mut contrast = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..HIGHCONTRASTW::default()
    };
    // SAFETY: `contrast` is initialized with the required size and writable for this call.
    unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            contrast.cbSize,
            Some((&raw mut contrast).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .is_ok()
        && contrast.dwFlags.contains(HCF_HIGHCONTRASTON)
}

fn system_prefers_dark() -> bool {
    read_current_user_dword(PERSONALIZE_KEY, APPS_USE_LIGHT_THEME) == Some(0)
}

fn read_current_user_dword(subkey: &str, name: &str) -> Option<u32> {
    let subkey = wide_null(subkey);
    let name = wide_null(name);
    let mut handle = HKEY::default();
    // SAFETY: the UTF-16 key is NUL-terminated and `handle` receives one owned registry handle.
    if unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(subkey.as_ptr()),
            Some(0),
            KEY_READ,
            &mut handle,
        )
    } != ERROR_SUCCESS
    {
        return None;
    }
    let key = RegistryKey(handle);
    let mut value_type = REG_VALUE_TYPE(0);
    let mut value = 0_u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: the DWORD buffer and byte count match; the key is live through this query.
    let status = unsafe {
        RegQueryValueExW(
            key.0,
            PCWSTR(name.as_ptr()),
            None,
            Some(&mut value_type),
            Some((&raw mut value).cast()),
            Some(&mut size),
        )
    };
    (status == ERROR_SUCCESS && value_type == REG_DWORD && size == size_of::<u32>() as u32)
        .then_some(value)
}

fn system_contrast_colors() -> SystemContrastColors {
    SystemContrastColors {
        canvas: system_color(COLOR_WINDOW),
        panel: system_color(COLOR_BTNFACE),
        text: system_color(COLOR_WINDOWTEXT),
        muted_text: system_color(COLOR_GRAYTEXT),
        accent: system_color(COLOR_HIGHLIGHT),
        accent_text: system_color(COLOR_HIGHLIGHTTEXT),
    }
}

fn system_color(index: windows::Win32::Graphics::Gdi::SYS_COLOR_INDEX) -> Color {
    // SAFETY: stock system colors require no resource ownership.
    let value = unsafe { GetSysColor(index) };
    Color {
        red: (value & 0xff) as u8,
        green: ((value >> 8) & 0xff) as u8,
        blue: ((value >> 16) & 0xff) as u8,
        alpha: 255,
    }
}

#[derive(Debug)]
struct RegistryKey(HKEY);

impl Drop for RegistryKey {
    fn drop(&mut self) {
        if self.0 != HKEY::default() {
            // SAFETY: this RAII wrapper closes its owned registry handle exactly once.
            let _ = unsafe { RegCloseKey(self.0) };
            self.0 = HKEY::default();
        }
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}
