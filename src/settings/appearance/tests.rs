use std::fs;

use serde_json::json;
use tempfile::tempdir;

use super::*;

#[test]
fn missing_preferences_default_to_system_without_writing() -> Result<(), std::io::Error> {
    let directory = tempdir()?;
    let path = directory.path().join(UI_PREFERENCES_FILE_NAME);
    let health = ui_preferences_health_from_path(Some(path.clone()));
    assert_eq!(health, UiPreferencesHealth::Missing);
    assert_eq!(health.effective_theme(), AppearanceThemePreference::System);
    assert!(!path.exists());
    Ok(())
}

#[test]
fn preference_round_trips_each_supported_theme() -> Result<(), std::io::Error> {
    let directory = tempdir()?;
    let path = directory.path().join(UI_PREFERENCES_FILE_NAME);
    for theme in [
        AppearanceThemePreference::System,
        AppearanceThemePreference::Light,
        AppearanceThemePreference::Dark,
    ] {
        persist_appearance_theme_from_path(Some(path.clone()), theme).map_err(debug_error)?;
        assert_eq!(
            ui_preferences_health_from_path(Some(path.clone())),
            UiPreferencesHealth::Available(theme)
        );
    }
    Ok(())
}

#[test]
fn parser_rejects_untrusted_shapes_without_repairing() -> Result<(), std::io::Error> {
    let directory = tempdir()?;
    let path = directory.path().join(UI_PREFERENCES_FILE_NAME);
    for (payload, expected) in [
        (
            serde_json::to_vec(&json!({"schemaVersion": 1, "theme": "sepia"}))?,
            UiPreferencesError::InvalidJson,
        ),
        (
            serde_json::to_vec(&json!({"schemaVersion": 2, "theme": "dark"}))?,
            UiPreferencesError::UnsupportedSchema,
        ),
        (
            serde_json::to_vec(&json!({
                "schemaVersion": 1,
                "theme": "dark",
                "unexpected": true
            }))?,
            UiPreferencesError::InvalidJson,
        ),
        (
            vec![b'x'; MAX_UI_PREFERENCES_BYTES + 1],
            UiPreferencesError::Oversize,
        ),
    ] {
        fs::write(&path, payload)?;
        assert_eq!(
            ui_preferences_health_from_path(Some(path.clone())),
            UiPreferencesHealth::RepairRequired(expected)
        );
        assert!(path.exists());
    }
    Ok(())
}

#[test]
fn failed_write_preserves_the_last_valid_preference() -> Result<(), std::io::Error> {
    let directory = tempdir()?;
    let path = directory.path().join(UI_PREFERENCES_FILE_NAME);
    persist_appearance_theme_from_path(Some(path.clone()), AppearanceThemePreference::Light)
        .map_err(debug_error)?;
    let temporary = path.with_extension("json.tmp");
    fs::create_dir(&temporary)?;
    assert_eq!(
        persist_appearance_theme_from_path(Some(path.clone()), AppearanceThemePreference::Dark),
        Err(UiPreferencesError::Io)
    );
    assert_eq!(
        ui_preferences_health_from_path(Some(path.clone())),
        UiPreferencesHealth::Available(AppearanceThemePreference::Light)
    );
    fs::remove_dir(temporary)?;
    assert_eq!(
        store::write_with_failed_replace(
            &path,
            UiPreferences {
                schema_version: UI_PREFERENCES_SCHEMA_VERSION,
                theme: AppearanceThemePreference::Dark,
            },
        ),
        Err(UiPreferencesError::Io)
    );
    assert_eq!(
        ui_preferences_health_from_path(Some(path.clone())),
        UiPreferencesHealth::Available(AppearanceThemePreference::Light)
    );
    assert!(!path.with_extension("json.tmp").exists());
    Ok(())
}

fn debug_error(error: UiPreferencesError) -> std::io::Error {
    std::io::Error::other(format!("{error:?}"))
}
