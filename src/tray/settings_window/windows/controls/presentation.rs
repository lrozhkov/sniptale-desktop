//! Projection of staged settings into the adaptive native control tree.

use super::*;
use crate::{
    settings::{
        AppearanceThemePreference, ManualAudioSource, ManualFps, ManualHardwarePreference,
        ManualVideoQuality,
    },
    tray::settings_window::model::SettingsWindowFeedback,
};
use windows::Win32::UI::Controls::{BST_CHECKED, BST_UNCHECKED};

pub(super) fn refresh(window: &mut SettingsNativeWindow) -> io::Result<()> {
    native::set_text(
        window.shell.headline,
        copy::headline(window.presentation.view()),
    )?;
    native::set_text(window.shell.body, copy::body(window.presentation.view()))?;
    native::set_text(window.shell.apply, window.presentation.apply_label())?;
    native::set_text(
        window.shell.landing_badge,
        copy::landing_badge(window.presentation.view()),
    )?;
    native::set_text(
        window.shell.landing_title,
        copy::landing_title(window.presentation.view()),
    )?;
    native::set_text(
        window.shell.landing_body,
        copy::landing_body(window.presentation.view()),
    )?;
    native::set_text(
        window.shell.landing_note,
        copy::landing_note(window.presentation.view()),
    )?;
    fill_theme(window.shell.theme, window.presentation.theme());
    let manual = window.presentation.manual().cloned();
    set_content_visibility(window);
    set_confirmation_visibility(window, window.presentation.confirm_discard());
    native::set_enabled(window.shell.apply, window.presentation.can_apply());
    native::set_text(
        window.shell.feedback,
        feedback_copy(window.presentation.feedback()),
    )?;
    if let Some(manual) = manual {
        refresh_manual(window, &manual);
    }
    layout::apply(window);
    native::invalidate(window.handle);
    Ok(())
}

fn set_content_visibility(window: &SettingsNativeWindow) {
    let manual = window.presentation.shows_navigation();
    let landing = !manual;
    for (_, handle) in &window.shell.navigation {
        native::set_visible(*handle, manual);
    }
    native::set_visible(window.shell.extension_mode, manual);
    for handle in [
        window.shell.landing_badge,
        window.shell.landing_title,
        window.shell.landing_body,
        window.shell.landing_note,
    ] {
        native::set_visible(handle, landing);
    }
    for handle in window.pages.all_handles() {
        native::set_visible(handle, false);
    }
    if manual {
        for handle in window.pages.page_handles(window.presentation.section()) {
            native::set_visible(*handle, true);
        }
    } else {
        native::set_visible(
            window.pages.capture.browse,
            window.presentation.shows_manual_setup(),
        );
        let _ = native::set_text(window.pages.capture.browse, "Set up Manual mode…");
    }
    native::set_visible(window.shell.apply, window.presentation.shows_footer());
    native::set_visible(window.shell.cancel, window.presentation.shows_footer());
}

fn set_confirmation_visibility(window: &SettingsNativeWindow, visible: bool) {
    for handle in [
        window.shell.confirm_text,
        window.shell.confirm_discard,
        window.shell.keep_editing,
    ] {
        native::set_visible(handle, visible);
    }
    let show_actions = !visible && window.presentation.shows_footer();
    native::set_visible(window.shell.apply, show_actions);
    native::set_visible(window.shell.cancel, show_actions);
    native::set_visible(
        window.shell.feedback,
        !visible && window.presentation.feedback().is_some(),
    );
}

fn refresh_manual(window: &SettingsNativeWindow, manual: &crate::settings::ManualProfileDraftView) {
    let _ = native::set_text(window.pages.capture.browse, "Change folder…");
    let _ = native::set_text(
        window.pages.capture.destination_path,
        &manual.destination_path,
    );
    set_check(
        window.pages.capture.screenshot_cursor,
        manual.screenshot_include_cursor,
    );
    set_check(window.pages.video.cursor, manual.video_include_cursor);
    refresh_video(window, manual);
    refresh_audio(window, manual);
    refresh_hotkeys(window);
}

fn refresh_video(window: &SettingsNativeWindow, manual: &crate::settings::ManualProfileDraftView) {
    native::set_combo_items(
        window.pages.video.quality,
        &["Low", "Balanced", "High", "Ultra"],
        quality_index(manual.video_quality),
    );
    native::set_combo_items(
        window.pages.video.fps,
        &["24 FPS", "30 FPS", "60 FPS"],
        fps_index(manual.video_fps),
    );
    let bitrates = window.presentation.video_bitrate_options();
    let bitrate_labels = bitrates
        .iter()
        .map(|option| option.label())
        .collect::<Vec<_>>();
    let selected_bitrate = bitrates
        .iter()
        .position(|option| option.value() == manual.video_bitrate_override_mbps)
        .unwrap_or(0);
    native::set_combo_items(
        window.pages.video.bitrate,
        &bitrate_labels,
        selected_bitrate,
    );
    native::set_combo_items(
        window.pages.video.hardware,
        &["Prefer hardware", "Software only"],
        hardware_index(manual.video_hardware_preference),
    );
    let durations = window.presentation.video_duration_options();
    let duration_labels = durations
        .iter()
        .map(|option| option.label())
        .collect::<Vec<_>>();
    let selected_duration = durations
        .iter()
        .position(|option| option.value() == manual.video_max_duration_seconds)
        .unwrap_or(0);
    native::set_combo_items(
        window.pages.video.duration,
        &duration_labels,
        selected_duration,
    );
}

fn refresh_audio(window: &SettingsNativeWindow, manual: &crate::settings::ManualProfileDraftView) {
    let sources = window.presentation.audio_sources();
    let source_labels = sources
        .iter()
        .map(|option| option.label())
        .collect::<Vec<_>>();
    let selected_source = sources
        .iter()
        .position(|option| option.source() == manual.audio_source_mode)
        .unwrap_or(0);
    native::set_combo_items(window.pages.audio.source, &source_labels, selected_source);
    native::set_enabled(
        window.pages.audio.source,
        sources.iter().any(|option| option.available()),
    );
    let microphones = window
        .presentation
        .microphones()
        .iter()
        .map(|option| option.label())
        .collect::<Vec<_>>();
    native::set_combo_items(
        window.pages.audio.microphone,
        &microphones,
        window.presentation.selected_microphone_index(),
    );
    native::set_enabled(
        window.pages.audio.microphone,
        !matches!(manual.audio_source_mode, ManualAudioSource::System) && !microphones.is_empty(),
    );
    native::set_combo_items(
        window.pages.audio.bitrate,
        &["96 kbps", "128 kbps", "160 kbps", "192 kbps"],
        audio_bitrate_index(manual.audio_bitrate_kbps),
    );
}

pub(super) fn refresh_hotkeys(window: &SettingsNativeWindow) {
    let hotkeys = window.presentation.manual().map(|manual| &manual.hotkeys);
    for (action, _, control) in &window.pages.shortcuts.hotkeys {
        let label = hotkeys
            .and_then(|hotkeys| hotkeys.get(action.as_str()))
            .and_then(|value| crate::settings::parse_manual_hotkey_chord(value))
            .map_or_else(|| "Not set".to_owned(), |chord| chord.canonical_label());
        let _ = native::set_text(*control, &label);
        native::invalidate(*control);
    }
}

fn fill_theme(control: HWND, theme: AppearanceThemePreference) {
    native::set_combo_items(
        control,
        &["System theme", "Light", "Dark"],
        match theme {
            AppearanceThemePreference::System => 0,
            AppearanceThemePreference::Light => 1,
            AppearanceThemePreference::Dark => 2,
        },
    );
}

fn feedback_copy(feedback: Option<SettingsWindowFeedback>) -> &'static str {
    match feedback {
        None => "",
        Some(SettingsWindowFeedback::AppearanceSaveFailed) => "Theme could not be saved.",
        Some(SettingsWindowFeedback::SaveFailed) => "Changes could not be saved. Try again.",
        Some(SettingsWindowFeedback::RecordingBusy) => {
            "Stop the active recording before changing mode."
        }
        Some(SettingsWindowFeedback::HotkeyUnavailable) => {
            "A shortcut is already reserved by Windows or another app."
        }
        Some(SettingsWindowFeedback::MicrophoneUnavailable) => {
            "Choose an available microphone or System default."
        }
        Some(SettingsWindowFeedback::AudioSourceUnavailable) => {
            "That audio source is unavailable on this device."
        }
        Some(SettingsWindowFeedback::HotkeyConflict { .. }) => {
            "That shortcut is already assigned to another action."
        }
    }
}

fn set_check(handle: HWND, checked: bool) {
    native::set_checked(
        handle,
        if checked {
            BST_CHECKED.0 as usize
        } else {
            BST_UNCHECKED.0 as usize
        },
    );
}

const fn quality_index(value: ManualVideoQuality) -> usize {
    match value {
        ManualVideoQuality::Low => 0,
        ManualVideoQuality::Medium => 1,
        ManualVideoQuality::High => 2,
        ManualVideoQuality::Ultra => 3,
    }
}

const fn fps_index(value: ManualFps) -> usize {
    match value {
        ManualFps::Fps24 => 0,
        ManualFps::Fps30 => 1,
        ManualFps::Fps60 => 2,
    }
}

const fn hardware_index(value: ManualHardwarePreference) -> usize {
    match value {
        ManualHardwarePreference::Prefer => 0,
        ManualHardwarePreference::ForceSoftware => 1,
    }
}

const fn audio_bitrate_index(value: u16) -> usize {
    match value {
        96 => 0,
        160 => 2,
        192 => 3,
        _ => 1,
    }
}
