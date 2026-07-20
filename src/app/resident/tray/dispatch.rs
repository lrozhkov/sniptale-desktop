//! First-party Manual tray action dispatch.

#[cfg(any(windows, test))]
#[path = "dispatch/settings.rs"]
mod settings;
#[cfg(any(windows, test))]
pub(super) use settings::dispatch_settings_event;
mod extension_intent;
pub(super) mod mode;
pub(super) use mode::{open_extension_managed, open_manual_setup};
pub(super) mod quit;
mod safety;

use crate::{
    app::resident::{
        AgentEventHub, AgentShutdownSignal, control::manual_dispatch::ManualActionDispatcher,
    },
    protocol::{RecordingControl, ScreenshotMode, VideoMode},
    tray::{
        ShellActionId, TrayActionKind, TrayCommandSource, TrayMenuCommand, TrayNoticeKind,
        plan_open_settings_fallback,
    },
};

use super::super::events::AgentEventPublishResult;

pub(super) struct TrayDispatchContext<'a> {
    pub(super) active_settings: &'a crate::settings::ActiveSettings,
    pub(super) manual_dispatcher: Option<&'a mut ManualActionDispatcher>,
    pub(super) registry: Option<&'a crate::tray::TrayActionRegistry>,
    pub(super) manual_senders: &'a super::ManualWorkerSenders,
    pub(super) connected: bool,
    pub(super) events: &'a AgentEventHub,
    pub(super) shutdown_sender: &'a std::sync::mpsc::SyncSender<AgentShutdownSignal>,
}

pub(super) fn open_settings(
    active_settings: &crate::settings::ActiveSettings,
    model: &mut crate::tray::settings_window::model::SettingsWindowModel,
    source: &mut impl TrayCommandSource,
) {
    let result = model
        .show(
            active_settings,
            crate::settings::ui_preferences_health_from_path(
                crate::app::platform::ui_preferences_path(),
            ),
        )
        .map_err(|_| crate::tray::TrayRuntimeError::RenderFailed)
        .map(|_| {
            model.refresh_audio_capabilities(crate::audio::runtime_audio_capability_summary());
            model.presentation()
        })
        .and_then(|presentation| source.show_settings_window(presentation));
    if result.is_err() {
        let _ = source.show_notice(TrayNoticeKind::ActionRejected);
    }
}

pub(super) fn dispatch_command(
    command: &TrayMenuCommand,
    context: TrayDispatchContext<'_>,
    source: &mut impl TrayCommandSource,
) {
    let TrayDispatchContext {
        active_settings,
        manual_dispatcher,
        registry,
        manual_senders,
        connected,
        events,
        shutdown_sender,
    } = context;
    if command.action_id == ShellActionId::OpenSaveFolder.as_str() {
        open_save_folder(active_settings, source);
        return;
    }
    if let Some(dispatcher) = manual_dispatcher
        && dispatch_manual_command(&command.action_id, dispatcher, source)
    {
        return;
    }
    if let Some(control) = safety::control_for_action(&command.action_id) {
        safety::dispatch(control, manual_senders, source);
        return;
    }
    if let (Some(registry), Some(shell_action)) = (
        registry,
        ShellActionId::from_extension_action_id(&command.action_id),
    ) {
        extension_intent::dispatch(shell_action, registry, connected, events, source);
        return;
    }
    match command.action_kind {
        TrayActionKind::OpenSettings => {
            show_notice_if_not_queued(source, events.publish_open_settings(), true);
        }
        TrayActionKind::OpenSaveFolder => {}
        TrayActionKind::Quit => {
            let _ = shutdown_sender.try_send(AgentShutdownSignal::Stop);
        }
        TrayActionKind::CaptureScreenshot
        | TrayActionKind::StartRecording
        | TrayActionKind::StopRecording
        | TrayActionKind::PauseRecording
        | TrayActionKind::ResumeRecording
        | TrayActionKind::OpenGallery
        | TrayActionKind::OpenVideoEditor => {
            show_notice_if_not_queued(
                source,
                events.publish_tray_menu_action(&command.action_id),
                false,
            );
        }
    }
}

fn open_save_folder(
    active_settings: &crate::settings::ActiveSettings,
    source: &mut impl TrayCommandSource,
) {
    let result = active_settings
        .profile_snapshot()
        .ok()
        .and_then(|profile| profile.manual_destination())
        .and_then(|destination| {
            crate::assets::ManualDestination::open(destination.id, destination.root).ok()
        })
        .map_or(
            Err(crate::tray::TrayRuntimeError::RenderFailed),
            |destination| source.open_save_folder(destination.root()),
        );
    if result.is_err() {
        let _ = source.show_notice(TrayNoticeKind::ActionRejected);
    }
}

fn show_notice_if_not_queued(
    source: &mut impl TrayCommandSource,
    publish_result: AgentEventPublishResult,
    open_settings: bool,
) {
    let notice = match publish_result {
        AgentEventPublishResult::Queued => None,
        AgentEventPublishResult::NoSubscriber if open_settings => {
            plan_open_settings_fallback(None).notice
        }
        AgentEventPublishResult::NoSubscriber => Some(TrayNoticeKind::ExtensionDisconnected),
        AgentEventPublishResult::QueueFull => Some(TrayNoticeKind::ActionRejected),
    };
    if let Some(notice) = notice {
        let _ = source.show_notice(notice);
    }
}

pub(super) fn dispatch_manual_command(
    action_id: &str,
    dispatcher: &mut ManualActionDispatcher,
    source: &mut impl TrayCommandSource,
) -> bool {
    let result = match action_id {
        id if id == ShellActionId::ScreenshotScreen.as_str() => {
            Some(dispatcher.enqueue_screenshot(ScreenshotMode::Screen))
        }
        id if id == ShellActionId::ScreenshotActiveWindow.as_str() => {
            Some(dispatcher.enqueue_screenshot(ScreenshotMode::ActiveWindow))
        }
        id if id == ShellActionId::ScreenshotAllScreens.as_str() => {
            Some(dispatcher.enqueue_screenshot(ScreenshotMode::AllScreens))
        }
        id if id == ShellActionId::ScreenshotRegion.as_str() => {
            Some(dispatcher.enqueue_screenshot(ScreenshotMode::Region))
        }
        id if id == ShellActionId::RecordingStartScreen.as_str() => {
            Some(dispatcher.enqueue_recording(VideoMode::Screen))
        }
        id if id == ShellActionId::RecordingStartActiveWindow.as_str() => {
            Some(dispatcher.enqueue_recording(VideoMode::ActiveWindow))
        }
        id if id == ShellActionId::RecordingStartRegion.as_str() => {
            Some(dispatcher.enqueue_recording(VideoMode::Region))
        }
        id if id == ShellActionId::RecordingPause.as_str() => {
            Some(dispatcher.control_recording(&RecordingControl::Pause))
        }
        id if id == ShellActionId::RecordingResume.as_str() => {
            Some(dispatcher.control_recording(&RecordingControl::Resume))
        }
        id if id == ShellActionId::RecordingStop.as_str() => {
            Some(dispatcher.control_recording(&RecordingControl::Stop))
        }
        id if id == ShellActionId::RecordingCancel.as_str() => {
            Some(dispatcher.control_recording(&RecordingControl::Cancel))
        }
        id if id == ShellActionId::Quit.as_str() => None,
        _ => {
            let _ = source.show_notice(TrayNoticeKind::ActionRejected);
            return true;
        }
    };
    if result.is_some_and(|result| result.is_err()) {
        let _ = source.show_notice(TrayNoticeKind::ActionRejected);
    }
    result.is_some()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::tempdir;

    use super::open_save_folder;
    use crate::{
        settings::{
            ActiveSettings, LocalProfileDraft, ManualProfileDraft,
            apply_local_profile_draft_from_path, ensure_local_profile_from_path,
        },
        tray::{
            TrayCommandSource, TrayIconVariant, TrayMenuRenderPlan, TrayNoticeKind,
            TrayRuntimeError, TrayRuntimeEvent,
        },
    };

    #[derive(Default)]
    struct Source {
        opened: bool,
        notices: Vec<TrayNoticeKind>,
    }

    impl TrayCommandSource for Source {
        fn apply_render_plan(
            &mut self,
            _plan: &TrayMenuRenderPlan,
            _tooltip: &str,
            _icon: TrayIconVariant,
        ) -> Result<(), TrayRuntimeError> {
            Ok(())
        }

        fn drain_events(&mut self) -> Result<Vec<TrayRuntimeEvent>, TrayRuntimeError> {
            Ok(Vec::new())
        }

        fn show_notice(&mut self, kind: TrayNoticeKind) -> Result<(), TrayRuntimeError> {
            self.notices.push(kind);
            Ok(())
        }

        fn show_settings_window(
            &mut self,
            _view: crate::tray::settings_window::model::SettingsWindowPresentation,
        ) -> Result<(), TrayRuntimeError> {
            Ok(())
        }

        fn hide_settings_window(&mut self) -> Result<(), TrayRuntimeError> {
            Ok(())
        }

        #[cfg(windows)]
        fn pick_settings_destination(
            &mut self,
        ) -> Result<Option<std::path::PathBuf>, TrayRuntimeError> {
            Ok(None)
        }

        fn open_save_folder(&mut self, _destination: &Path) -> Result<(), TrayRuntimeError> {
            self.opened = true;
            Ok(())
        }
    }

    #[test]
    fn manual_destination_opens_without_emitting_its_path() -> Result<(), Box<dyn std::error::Error>>
    {
        let directory = tempdir()?;
        let profile_path = directory.path().join("local-profile.json");
        let profile = ensure_local_profile_from_path(Some(profile_path.clone()))
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        let settings = ActiveSettings::from_profile(&profile);
        let draft = LocalProfileDraft::manual(ManualProfileDraft::with_defaults(
            "destination".to_owned(),
            directory.path().to_string_lossy().into_owned(),
        ));
        apply_local_profile_draft_from_path(Some(profile_path), &settings, &draft)
            .map_err(|error| std::io::Error::other(format!("{error:?}")))?;
        let mut source = Source::default();

        open_save_folder(&settings, &mut source);

        assert!(source.opened);
        assert!(source.notices.is_empty());
        Ok(())
    }
}
