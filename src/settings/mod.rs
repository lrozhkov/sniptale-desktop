//! Accepted extension settings snapshots, effective settings, and technical cache.

mod actions;
mod active;
#[cfg(any(windows, test))]
mod browser;
#[cfg(all(windows, not(test)))]
pub(crate) use browser::startup::{
    browser_start_allowed, browser_startup_preference_path, persist_browser_start_allowed,
};
#[cfg(any(windows, test))]
pub(crate) use browser::{BrowserTarget, BrowserTargetError, BrowserTargetRegistry};
#[cfg(any(windows, test))]
mod appearance;
mod cache;
#[cfg(any(windows, test))]
pub(crate) use profile::desktop_snapshot::desktop_settings_snapshot;
#[cfg(any(windows, test))]
mod draft;
pub(crate) mod effective;
mod manual;
mod mode;
mod profile;
mod profile_store;
mod snapshot;
#[cfg(test)]
mod tests;

pub(crate) use actions::{ActionCatalog, ActionCatalogError, action_catalog_from_native_actions};
pub(crate) use active::ActiveSettings;
#[cfg(any(windows, test))]
pub(crate) use active::SettingsApplyError;
#[cfg(any(windows, test))]
pub(crate) use appearance::{
    AppearancePreferences, AppearanceThemePreference, UiLocale, UiPreferencesHealth,
    load_ui_preferences_from_path, ui_preferences_file_name, ui_preferences_health_from_path,
    update_ui_preferences_from_path,
};
pub(crate) use cache::{
    CacheSchemaStatus, NativeCacheHealth, native_cache_file_name, native_cache_health_from_path,
};
#[cfg(any(windows, test))]
pub(crate) use draft::LocalProfileDraft;
pub(crate) use effective::effective_settings_for_sync;
#[cfg(any(windows, test))]
pub(crate) use manual::{ManualAudioSource, ManualHardwarePreference, ManualHotkeyAction};
#[cfg(test)]
pub(crate) use manual::{ManualFps, ManualHotkeyUpdateError, ManualVideoQuality};
#[cfg(any(windows, test))]
pub(crate) use manual::{ManualHotkeyChord, ManualHotkeyKey, parse_manual_hotkey_chord};
#[cfg(any(windows, test))]
pub(crate) use manual::{ManualProfileDraft, ManualProfileDraftView};
#[cfg(test)]
pub(crate) use mode::ModePhase;
pub(crate) use mode::{ModeEpoch, ModeTransitionError};
#[cfg(any(windows, test))]
pub(crate) use profile::LocalProfileError;
pub(crate) use profile::{
    LocalProfile, LocalProfileHealth, OperatingMode, extension_admission_allowed_from_path,
    local_profile_file_name, local_profile_health_from_path,
};
#[cfg(test)]
pub(crate) use profile_store::apply_local_profile_draft_from_path;
pub(crate) use profile_store::ensure_local_profile_from_path;
#[cfg(any(windows, test))]
pub(crate) use profile_store::load_local_profile_from_path;
#[cfg(any(windows, test))]
pub(crate) use profile_store::persist_profile_candidate_from_path;
pub(crate) use snapshot::{
    AcceptedRecordingTelemetrySettings, AcceptedSettingsSnapshot, SettingsSnapshotStore,
    max_recording_duration_ms_from_minutes,
};
