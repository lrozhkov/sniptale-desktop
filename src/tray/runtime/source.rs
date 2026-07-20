//! Tray runtime command source seam.

use std::path::PathBuf;
use std::{error::Error, fmt, path::Path};

#[cfg(any(windows, test))]
use crate::tray::settings_window::model::SettingsWindowEvent;
use crate::tray::{
    icon::state::{TrayIconVariant, TrayNoticeKind},
    menu::render::TrayMenuRenderPlan,
    settings_window::model::SettingsWindowPresentation,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TrayRuntimeEvent {
    #[cfg(any(windows, test))]
    MenuCommand { command_id: u32 },
    #[cfg(any(windows, test))]
    SettingsWindow { event: SettingsWindowEvent },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrayRuntimeError {
    RenderFailed,
    #[cfg(windows)]
    PollFailed,
}

impl fmt::Display for TrayRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RenderFailed => formatter.write_str("tray render failed"),
            #[cfg(windows)]
            Self::PollFailed => formatter.write_str("tray event polling failed"),
        }
    }
}

impl Error for TrayRuntimeError {}

pub(crate) trait TrayCommandSource {
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

/// Decodes a `NOTIFYICON_VERSION_4` callback without exposing Win32 types to the tray owner.
#[must_use]
pub(crate) const fn is_tray_popup_callback(lparam: isize) -> bool {
    matches!(lparam & 0xffff, 0x007b | 0x0202 | 0x0205 | 0x0400)
}

#[cfg(not(windows))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct NoopTrayCommandSource;

#[cfg(not(windows))]
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

#[cfg(test)]
mod tests {
    use super::is_tray_popup_callback;

    #[test]
    fn accepts_select_context_and_button_events_with_a_nonzero_icon_id() {
        assert!(is_tray_popup_callback((1_isize << 16) | 0x007b));
        assert!(is_tray_popup_callback((1_isize << 16) | 0x0202));
        assert!(is_tray_popup_callback((1_isize << 16) | 0x0205));
        assert!(is_tray_popup_callback((1_isize << 16) | 0x0400));
        assert!(!is_tray_popup_callback((1_isize << 16) | 0x0201));
    }
}
