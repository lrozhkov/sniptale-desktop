//! Tray runtime command source seam.

use std::path::PathBuf;
use std::{error::Error, fmt, path::Path};

use crate::tray::{
    icon::state::{TrayIconVariant, TrayNoticeKind},
    menu::render::TrayMenuRenderPlan,
    settings_window::model::SettingsWindowPresentation,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TrayRuntimeEvent {
    #[cfg(any(windows, test))]
    MenuCommand { command_id: u32 },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrayRuntimeError {
    RenderFailed,
    NoCaptureWindow,
    #[cfg(windows)]
    PollFailed,
}

impl fmt::Display for TrayRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoCaptureWindow => formatter.write_str("no capture window available"),
            Self::RenderFailed => formatter.write_str("tray render failed"),
            #[cfg(windows)]
            Self::PollFailed => formatter.write_str("tray event polling failed"),
        }
    }
}

impl Error for TrayRuntimeError {}

pub(crate) trait TrayCommandSource {
    #[cfg(windows)]
    fn attach_desktop(&mut self, _desktop: crate::tray::desktop::DesktopHandle) {}

    #[cfg(windows)]
    fn capture_window_available(&self) -> bool {
        false
    }

    fn finish_capture_dispatch(&mut self, _accepted: bool) {}

    /// Settle native presentation before an admitted capture. Media admission stays in its owner.
    fn prepare_capture(
        &mut self,
        _action: crate::tray::ShellActionId,
    ) -> Result<(), TrayRuntimeError> {
        Ok(())
    }

    fn choose_delivery_destination(
        &mut self,
        _request: &crate::tray::delivery::DeliveryPromptRequest,
    ) -> crate::tray::delivery::DeliveryChoice {
        crate::tray::delivery::DeliveryChoice::Later
    }
    fn apply_render_plan(
        &mut self,
        plan: &TrayMenuRenderPlan,
        tooltip: &str,
        icon: TrayIconVariant,
    ) -> Result<(), TrayRuntimeError>;

    fn drain_events(&mut self) -> Result<Vec<TrayRuntimeEvent>, TrayRuntimeError>;

    fn show_notice(&mut self, kind: TrayNoticeKind) -> Result<(), TrayRuntimeError>;

    fn confirm_stop_recording_and_exit(&mut self) -> Result<bool, TrayRuntimeError> {
        Ok(false)
    }

    fn show_settings_window(
        &mut self,
        presentation: SettingsWindowPresentation,
    ) -> Result<(), TrayRuntimeError>;

    #[cfg(any(windows, test))]
    fn hide_settings_window(&mut self) -> Result<(), TrayRuntimeError>;

    fn pick_settings_destination(&mut self) -> Result<Option<PathBuf>, TrayRuntimeError> {
        Ok(None)
    }

    /// Opens a previously validated Manual destination without exposing its path to logs/events.
    fn open_save_folder(&mut self, destination: &Path) -> Result<(), TrayRuntimeError>;
}

#[cfg(any(not(windows), test))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct NoopTrayCommandSource;

#[cfg(any(not(windows), test))]
impl TrayCommandSource for NoopTrayCommandSource {
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

    fn show_notice(&mut self, _kind: TrayNoticeKind) -> Result<(), TrayRuntimeError> {
        Ok(())
    }

    fn show_settings_window(
        &mut self,
        _presentation: SettingsWindowPresentation,
    ) -> Result<(), TrayRuntimeError> {
        Ok(())
    }

    #[cfg(any(windows, test))]
    fn hide_settings_window(&mut self) -> Result<(), TrayRuntimeError> {
        Ok(())
    }

    fn pick_settings_destination(&mut self) -> Result<Option<PathBuf>, TrayRuntimeError> {
        Ok(None)
    }

    fn open_save_folder(&mut self, _destination: &Path) -> Result<(), TrayRuntimeError> {
        Ok(())
    }
}
