//! Settings-window event coordination for the tray worker.

use crate::{
    app::resident::{
        control::{
            manual_dispatch::ManualActionDispatcher,
            mode_coordinator::{
                ModeCoordinatorError, ModeTransitionBarrier, apply_profile_draft_with_barrier,
            },
        },
        tray::ManualWorkerSenders,
    },
    tray::{
        TrayCommandSource, TrayRecordingStatus,
        settings_window::model::{
            CloseDisposition, SettingsWindowEvent, SettingsWindowFeedback, SettingsWindowModel,
        },
    },
};

pub(crate) fn dispatch_settings_event(
    event: SettingsWindowEvent,
    active_settings: &crate::settings::ActiveSettings,
    model: &mut SettingsWindowModel,
    manual_dispatcher: &mut Option<ManualActionDispatcher>,
    manual_senders: &ManualWorkerSenders,
    recording_status: TrayRecordingStatus,
    source: &mut impl TrayCommandSource,
) -> bool {
    dispatch_settings_event_with_paths(
        event,
        SettingsEventContext {
            active_settings,
            model,
            manual_dispatcher,
            manual_senders,
            recording_status,
            source,
            paths: SettingsPersistencePaths::runtime(),
        },
    )
}

struct SettingsEventContext<'a, Source> {
    active_settings: &'a crate::settings::ActiveSettings,
    model: &'a mut SettingsWindowModel,
    manual_dispatcher: &'a mut Option<ManualActionDispatcher>,
    manual_senders: &'a ManualWorkerSenders,
    recording_status: TrayRecordingStatus,
    source: &'a mut Source,
    paths: SettingsPersistencePaths,
}

fn dispatch_settings_event_with_paths<Source: TrayCommandSource>(
    event: SettingsWindowEvent,
    context: SettingsEventContext<'_, Source>,
) -> bool {
    let SettingsEventContext {
        active_settings,
        model,
        manual_dispatcher,
        manual_senders,
        recording_status,
        source,
        paths,
    } = context;
    match event {
        SettingsWindowEvent::Navigate(section) => model.navigate(section),
        SettingsWindowEvent::BrowseDestination => stage_settings_destination(model, source),
        SettingsWindowEvent::ToggleScreenshotCursor => {
            let include_cursor = model
                .manual_view()
                .is_some_and(|manual| !manual.screenshot_include_cursor);
            model.set_screenshot_include_cursor(include_cursor);
        }
        SettingsWindowEvent::SetVideoQuality(value) => model.set_video_quality(value),
        SettingsWindowEvent::SetVideoFps(value) => model.set_video_fps(value),
        SettingsWindowEvent::SetVideoBitrate(value) => model.set_video_bitrate(value),
        SettingsWindowEvent::SetVideoHardware(value) => model.set_video_hardware(value),
        SettingsWindowEvent::ToggleVideoCursor => model.toggle_video_cursor(),
        SettingsWindowEvent::SetVideoDuration(value) => model.set_video_duration(value),
        SettingsWindowEvent::SetAudioSource(value) => model.set_audio_source(value),
        SettingsWindowEvent::SelectMicrophone(selection) => model.select_microphone(selection),
        SettingsWindowEvent::SetAudioBitrate(value) => model.set_audio_bitrate(value),
        SettingsWindowEvent::SetHotkey { action, chord } => model.set_hotkey(action, chord),
        SettingsWindowEvent::CancelHotkeyCapture(action) => model.cancel_hotkey_edit(action),
        SettingsWindowEvent::RefreshMicrophones => refresh_audio_capabilities(model),
        SettingsWindowEvent::SelectAppearanceTheme(theme) => {
            if crate::settings::persist_appearance_theme_from_path(paths.appearance, theme).is_ok()
            {
                model.appearance_saved(theme);
            } else {
                model.appearance_save_failed();
            }
        }
        SettingsWindowEvent::UseExtensionManaged => model.stage_extension_managed(),
        SettingsWindowEvent::Apply => {
            return apply_settings_draft(
                active_settings,
                model,
                manual_dispatcher,
                manual_senders,
                recording_status,
                source,
                paths.profile,
            );
        }
        SettingsWindowEvent::RequestClose => {
            if model.request_close() == CloseDisposition::Hide {
                let _ = source.hide_settings_window();
                return false;
            }
        }
        SettingsWindowEvent::ConfirmDiscard => {
            model.discard_and_hide();
            let _ = source.hide_settings_window();
            return false;
        }
        SettingsWindowEvent::KeepEditing => model.keep_editing(),
    }
    refresh_window(model, source);
    false
}

fn refresh_window(model: &SettingsWindowModel, source: &mut impl TrayCommandSource) {
    let _ = source.show_settings_window(model.presentation());
}

fn refresh_audio_capabilities(model: &mut SettingsWindowModel) {
    model.refresh_audio_capabilities(crate::audio::runtime_audio_capability_summary());
}

fn stage_settings_destination(
    model: &mut SettingsWindowModel,
    source: &mut impl TrayCommandSource,
) {
    if let Ok(Some(destination)) = source.pick_settings_destination() {
        model.stage_manual_destination(
            "manual-destination".to_owned(),
            destination.to_string_lossy().into_owned(),
        );
    }
}

fn apply_settings_draft(
    active_settings: &crate::settings::ActiveSettings,
    model: &mut SettingsWindowModel,
    manual_dispatcher: &mut Option<ManualActionDispatcher>,
    manual_senders: &ManualWorkerSenders,
    recording_status: TrayRecordingStatus,
    source: &mut impl TrayCommandSource,
    profile_path: Option<std::path::PathBuf>,
) -> bool {
    let Ok(draft) = model.apply_candidate() else {
        model.set_apply_feedback(SettingsWindowFeedback::SaveFailed);
        refresh_window(model, source);
        return false;
    };
    let mut barrier = TrayModeBarrier {
        recording_status,
        manual_senders,
        manual_hotkeys: draft.manual_view().map(|manual| manual.hotkeys),
        failure: None,
    };
    let result =
        apply_profile_draft_with_barrier(profile_path, active_settings, &draft, &mut barrier);
    match result {
        Ok(profile) => finish_apply(
            &profile,
            active_settings,
            model,
            manual_dispatcher,
            manual_senders,
            source,
        ),
        Err(_error) => {
            model.set_apply_feedback(apply_feedback(barrier.failure));
            refresh_window(model, source);
            false
        }
    }
}

#[derive(Clone, Debug)]
struct SettingsPersistencePaths {
    profile: Option<std::path::PathBuf>,
    appearance: Option<std::path::PathBuf>,
}

impl SettingsPersistencePaths {
    fn runtime() -> Self {
        Self {
            profile: crate::app::platform::local_profile_path(),
            appearance: crate::app::platform::ui_preferences_path(),
        }
    }
}

fn finish_apply(
    profile: &crate::settings::LocalProfile,
    active_settings: &crate::settings::ActiveSettings,
    model: &mut SettingsWindowModel,
    manual_dispatcher: &mut Option<ManualActionDispatcher>,
    manual_senders: &ManualWorkerSenders,
    source: &mut impl TrayCommandSource,
) -> bool {
    match profile.operating_mode() {
        crate::settings::OperatingMode::Manual if manual_dispatcher.is_none() => {
            *manual_dispatcher = Some(manual_senders.dispatcher(active_settings.clone()));
        }
        crate::settings::OperatingMode::ExtensionManaged => *manual_dispatcher = None,
        crate::settings::OperatingMode::Manual => {}
    }
    model.discard_and_hide();
    let _ = source.hide_settings_window();
    true
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BarrierFailure {
    RecordingBusy,
    HotkeyUnavailable,
    RuntimeUnavailable,
}

struct TrayModeBarrier<'a> {
    recording_status: TrayRecordingStatus,
    manual_senders: &'a ManualWorkerSenders,
    manual_hotkeys: Option<std::collections::BTreeMap<String, String>>,
    failure: Option<BarrierFailure>,
}

impl ModeTransitionBarrier for TrayModeBarrier<'_> {
    fn quiesce(
        &mut self,
        _switching: crate::settings::ModeEpoch,
    ) -> Result<(), ModeCoordinatorError> {
        if !matches!(self.recording_status, TrayRecordingStatus::Idle) {
            self.failure = Some(BarrierFailure::RecordingBusy);
            return Err(ModeCoordinatorError::Barrier);
        }
        self.manual_senders
            .quiesce_extension_inputs()
            .map_err(|()| {
                self.failure = Some(BarrierFailure::RuntimeUnavailable);
                ModeCoordinatorError::Barrier
            })
    }

    fn stage(
        &mut self,
        _switching: crate::settings::ModeEpoch,
    ) -> Result<(), ModeCoordinatorError> {
        if let Some(hotkeys) = &self.manual_hotkeys {
            self.manual_senders
                .apply_manual_hotkeys(hotkeys)
                .map_err(|_| {
                    self.failure = Some(BarrierFailure::HotkeyUnavailable);
                    ModeCoordinatorError::Barrier
                })?;
        }
        Ok(())
    }

    fn publish(&mut self, _stable: crate::settings::ModeEpoch) -> Result<(), ModeCoordinatorError> {
        Ok(())
    }

    fn rollback(
        &mut self,
        _stable: crate::settings::ModeEpoch,
    ) -> Result<(), ModeCoordinatorError> {
        self.manual_senders
            .restore_hotkeys()
            .map_err(|()| ModeCoordinatorError::Barrier)
    }
}

const fn apply_feedback(failure: Option<BarrierFailure>) -> SettingsWindowFeedback {
    match failure {
        Some(BarrierFailure::RecordingBusy) => SettingsWindowFeedback::RecordingBusy,
        Some(BarrierFailure::HotkeyUnavailable) => SettingsWindowFeedback::HotkeyUnavailable,
        Some(BarrierFailure::RuntimeUnavailable) | None => SettingsWindowFeedback::SaveFailed,
    }
}

#[cfg(test)]
#[path = "settings/tests.rs"]
mod tests;
