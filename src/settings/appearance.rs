//! Versioned native appearance preference and trust-boundary parsing.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[path = "appearance/store.rs"]
mod store;

const UI_PREFERENCES_FILE_NAME: &str = "ui-preferences.json";
const UI_PREFERENCES_SCHEMA_VERSION: u16 = 1;
const MAX_UI_PREFERENCES_BYTES: usize = 4 * 1024;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AppearanceThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub(crate) enum UiLocale {
    #[default]
    #[serde(rename = "en-US")]
    English,
    #[serde(rename = "ru")]
    Russian,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
pub(crate) struct AppearancePreferences {
    pub(crate) theme: AppearanceThemePreference,
    pub(crate) locale: UiLocale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UiPreferencesError {
    Io,
    Oversize,
    InvalidJson,
    UnsupportedSchema,
    EmptyPatch,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UiPreferencesHealth {
    Missing,
    Available(AppearanceThemePreference),
    RepairRequired(UiPreferencesError),
}

impl UiPreferencesHealth {
    #[must_use]
    pub(crate) const fn effective_theme(self) -> AppearanceThemePreference {
        match self {
            Self::Available(theme) => theme,
            Self::Missing | Self::RepairRequired(_) => AppearanceThemePreference::System,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UiPreferences {
    schema_version: u16,
    theme: AppearanceThemePreference,
    #[serde(default)]
    locale: UiLocale,
}

#[must_use]
pub(crate) fn ui_preferences_health_from_path(path: Option<PathBuf>) -> UiPreferencesHealth {
    match store::read(path) {
        Ok(Some(preferences)) => UiPreferencesHealth::Available(preferences.theme),
        Ok(None) => UiPreferencesHealth::Missing,
        Err(error) => UiPreferencesHealth::RepairRequired(error),
    }
}

#[cfg(test)]
fn persist_appearance_theme_from_path(
    path: Option<PathBuf>,
    theme: AppearanceThemePreference,
) -> Result<(), UiPreferencesError> {
    update_ui_preferences_from_path(path, Some(theme), None).map(|_| ())
}

pub(crate) fn load_ui_preferences_from_path(
    path: Option<PathBuf>,
) -> Result<AppearancePreferences, UiPreferencesError> {
    Ok(
        store::read(path)?.map_or_else(AppearancePreferences::default, |value| {
            AppearancePreferences {
                theme: value.theme,
                locale: value.locale,
            }
        }),
    )
}

pub(crate) fn update_ui_preferences_from_path(
    path: Option<PathBuf>,
    theme: Option<AppearanceThemePreference>,
    locale: Option<UiLocale>,
) -> Result<AppearancePreferences, UiPreferencesError> {
    if theme.is_none() && locale.is_none() {
        return Err(UiPreferencesError::EmptyPatch);
    }
    store::update(path, theme, locale)
}

fn parse(payload: &[u8]) -> Result<UiPreferences, UiPreferencesError> {
    if payload.len() > MAX_UI_PREFERENCES_BYTES {
        return Err(UiPreferencesError::Oversize);
    }
    let preferences = serde_json::from_slice::<UiPreferences>(payload)
        .map_err(|_| UiPreferencesError::InvalidJson)?;
    if preferences.schema_version != UI_PREFERENCES_SCHEMA_VERSION {
        return Err(UiPreferencesError::UnsupportedSchema);
    }
    Ok(preferences)
}

#[must_use]
pub(crate) const fn ui_preferences_file_name() -> &'static str {
    UI_PREFERENCES_FILE_NAME
}

#[cfg(test)]
#[path = "appearance/tests.rs"]
mod tests;
