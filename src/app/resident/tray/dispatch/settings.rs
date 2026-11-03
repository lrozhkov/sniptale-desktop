//! Settings-window event coordination for the tray worker.

use crate::tray::desktop::{model::DesktopError, settings::SettingsRequest};

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
        settings_window::model::{SettingsWindowFeedback, SettingsWindowModel},
    },
};

struct SettingsEventContext<'a, Source> {
    active_settings: &'a crate::settings::ActiveSettings,
    model: &'a mut SettingsWindowModel,
    manual_dispatcher: &'a mut Option<ManualActionDispatcher>,
    manual_senders: &'a ManualWorkerSenders,
    recording_status: TrayRecordingStatus,
    source: &'a mut Source,
    paths: SettingsPersistencePaths,
}

fn refresh_window(model: &SettingsWindowModel, source: &mut impl TrayCommandSource) {
    let _ = source.show_settings_window(model.presentation());
}

fn refresh_audio_capabilities(model: &mut SettingsWindowModel) {
    model.refresh_audio_capabilities(crate::audio::runtime_audio_capability_summary());
}

fn apply_settings_draft(
    context: SettingsEventContext<'_, impl TrayCommandSource>,
    keep_open: bool,
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
        apply_profile_draft_with_barrier(paths.profile, active_settings, &draft, &mut barrier);
    match result {
        Ok(profile) => finish_apply(
            &profile,
            active_settings,
            model,
            manual_dispatcher,
            manual_senders,
            source,
            keep_open,
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
    keep_open: bool,
) -> bool {
    let _ = profile;
    *manual_dispatcher = Some(manual_senders.dispatcher(active_settings.clone()));
    if keep_open {
        let theme = model.presentation().theme();
        let _ = model.show(
            active_settings,
            crate::settings::UiPreferencesHealth::Available(theme),
        );
        refresh_window(model, source);
    } else {
        model.discard_and_hide();
        let _ = source.hide_settings_window();
    }
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

pub(in crate::app::resident) fn dispatch_desktop_settings(
    request: crate::tray::desktop::settings::SettingsRequest,
    active_settings: &crate::settings::ActiveSettings,
    model: &mut SettingsWindowModel,
    manual_dispatcher: &mut Option<ManualActionDispatcher>,
    manual_senders: &ManualWorkerSenders,
    recording_status: TrayRecordingStatus,
    source: &mut impl TrayCommandSource,
) -> Result<
    crate::tray::desktop::settings::SettingsSnapshot,
    crate::tray::desktop::model::DesktopError,
> {
    dispatch_desktop_settings_with_context(
        request,
        SettingsEventContext {
            active_settings,
            model,
            manual_dispatcher,
            manual_senders,
            recording_status,
            source,
            paths: SettingsPersistencePaths::runtime(),
        },
        refresh_audio_capabilities,
    )
}

fn dispatch_desktop_settings_with_context(
    request: crate::tray::desktop::settings::SettingsRequest,
    context: SettingsEventContext<'_, impl TrayCommandSource>,
    refresh_audio: impl Fn(&mut SettingsWindowModel),
) -> Result<
    crate::tray::desktop::settings::SettingsSnapshot,
    crate::tray::desktop::model::DesktopError,
> {
    let SettingsEventContext {
        active_settings,
        model,
        manual_dispatcher,
        manual_senders,
        recording_status,
        source,
        paths,
    } = context;
    if matches!(request, SettingsRequest::Open {}) || !model.visible() {
        model
            .show(
                active_settings,
                crate::settings::ui_preferences_health_from_path(paths.appearance.clone()),
            )
            .map_err(|_| DesktopError::Unavailable)?;
        refresh_audio(model);
    }
    let mut selected_folder = None;
    match request {
        SettingsRequest::ChooseFolder {} => {
            if let Some(path) = source
                .pick_settings_destination()
                .map_err(|_| DesktopError::Unavailable)?
            {
                let path = path.to_string_lossy().into_owned();
                model.stage_manual_destination("manual-destination".to_owned(), path.clone());
                selected_folder = Some(path);
            }
        }
        SettingsRequest::RefreshDevices {} => refresh_audio(model),
        SettingsRequest::Open {} | SettingsRequest::RefreshBrowsers {} => {}
        SettingsRequest::SaveDelivery {
            target_id,
            ask_after_capture,
        } => {
            if recording_status != TrayRecordingStatus::Idle {
                return Err(DesktopError::RecordingBusy);
            }
            active_settings
                .select_browser_target(target_id.as_deref(), ask_after_capture)
                .map_err(|error| match error {
                    crate::settings::BrowserTargetError::Stale => DesktopError::StaleSettings,
                    _ => DesktopError::Unavailable,
                })?;
        }
        SettingsRequest::Apply {
            expected_revision,
            draft,
        } => {
            apply_desktop_profile(
                SettingsEventContext {
                    active_settings,
                    model,
                    manual_dispatcher,
                    manual_senders,
                    recording_status,
                    source,
                    paths,
                },
                expected_revision,
                draft,
                &refresh_audio,
            )?;
        }
    }
    let mut snapshot = desktop_settings_snapshot(active_settings, model)?;
    snapshot.selected_folder = selected_folder;
    Ok(snapshot)
}

fn apply_desktop_profile(
    context: SettingsEventContext<'_, impl TrayCommandSource>,
    expected_revision: u64,
    draft: crate::settings::ManualProfileDraftView,
    refresh_audio: &impl Fn(&mut SettingsWindowModel),
) -> Result<(), crate::tray::desktop::model::DesktopError> {
    let profile = context
        .active_settings
        .profile_snapshot()
        .map_err(|_| DesktopError::Unavailable)?;
    if profile.revision() != expected_revision {
        return Err(DesktopError::StaleSettings);
    }
    let destination_id = profile.manual_destination().map_or_else(
        || "manual-destination".to_owned(),
        |destination| destination.id,
    );
    let draft = crate::settings::ManualProfileDraft::from_view(draft, destination_id)
        .map_err(|_| DesktopError::InvalidSettings)?;
    context
        .model
        .stage_manual(crate::settings::LocalProfileDraft::manual(draft));
    refresh_audio(context.model);
    if context.model.dirty() {
        // The web surface stays open. The same barrier still owns runtime admission and rollback.
        let model = &mut *context.model;
        let applied = apply_settings_draft(
            SettingsEventContext {
                active_settings: context.active_settings,
                model,
                manual_dispatcher: context.manual_dispatcher,
                manual_senders: context.manual_senders,
                recording_status: context.recording_status,
                source: context.source,
                paths: context.paths,
            },
            true,
        );
        if !applied {
            return Err(match model.presentation().feedback() {
                Some(SettingsWindowFeedback::RecordingBusy) => DesktopError::RecordingBusy,
                Some(SettingsWindowFeedback::HotkeyUnavailable) => DesktopError::HotkeyUnavailable,
                _ if !model.presentation().can_apply() => DesktopError::MicrophoneUnavailable,
                _ => DesktopError::Unavailable,
            });
        }
    }
    refresh_audio(context.model);
    Ok(())
}

fn desktop_settings_snapshot(
    active_settings: &crate::settings::ActiveSettings,
    model: &SettingsWindowModel,
) -> Result<
    crate::tray::desktop::settings::SettingsSnapshot,
    crate::tray::desktop::model::DesktopError,
> {
    use crate::tray::{
        desktop::{
            model::DesktopError,
            settings::{
                AudioSourceOption, BrowserDelivery, BrowserDestination, MicrophoneOption,
                SettingsSnapshot,
            },
        },
        settings_window::model::SettingsMicrophoneSelection,
    };
    let profile = active_settings
        .profile_snapshot()
        .map_err(|_| DesktopError::Unavailable)?;
    let presentation = model.presentation();
    let registry = active_settings
        .browser_targets()
        .map_err(|_| DesktopError::Unavailable)?;
    Ok(SettingsSnapshot {
        profile_revision: profile.revision(),
        selected_folder: None,
        delivery: BrowserDelivery {
            targets: registry
                .targets()
                .iter()
                .map(|target| BrowserDestination {
                    id: target.id.clone(),
                    label: target.label(),
                })
                .collect(),
            target_id: registry.default_target_id().map(str::to_owned),
            ask_after_capture: registry.ask_after_capture(),
        },
        draft: model.manual_view().unwrap_or_else(|| {
            crate::settings::ManualProfileDraft::with_defaults(
                "manual-destination".to_owned(),
                String::new(),
            )
            .view()
        }),
        microphones: presentation
            .microphones()
            .iter()
            .map(|option| MicrophoneOption {
                id: match option.selection() {
                    SettingsMicrophoneSelection::SystemDefault => None,
                    SettingsMicrophoneSelection::Device(id) => Some(id),
                },
                name: option.name().to_owned(),
                missing: option.missing(),
                available: option.available(),
            })
            .collect(),
        audio_sources: presentation
            .audio_sources()
            .iter()
            .map(|option| AudioSourceOption {
                source: option.source(),
                available: option.available(),
            })
            .collect(),
    })
}

#[cfg(test)]
#[path = "settings/tests.rs"]
mod tests;
