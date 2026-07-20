//! Construction and semantic grouping of native child controls.

use super::*;
use crate::tray::settings_window::model::SettingsWindowSection;
use windows::Win32::UI::WindowsAndMessaging::{BS_OWNERDRAW, WS_TABSTOP};

#[path = "tree/pages.rs"]
mod pages;

#[derive(Debug)]
pub(super) struct ShellControls {
    pub(super) headline: HWND,
    pub(super) body: HWND,
    pub(super) theme_label: HWND,
    pub(super) theme: HWND,
    pub(super) navigation: Vec<(SettingsWindowSection, HWND)>,
    pub(super) extension_mode: HWND,
    pub(super) apply: HWND,
    pub(super) cancel: HWND,
    pub(super) feedback: HWND,
    pub(super) confirm_text: HWND,
    pub(super) confirm_discard: HWND,
    pub(super) keep_editing: HWND,
    pub(super) landing_badge: HWND,
    pub(super) landing_title: HWND,
    pub(super) landing_body: HWND,
    pub(super) landing_note: HWND,
}

impl ShellControls {
    pub(super) fn handles(&self) -> Vec<HWND> {
        let mut handles = vec![
            self.headline,
            self.body,
            self.theme_label,
            self.theme,
            self.extension_mode,
            self.apply,
            self.cancel,
            self.feedback,
            self.confirm_text,
            self.confirm_discard,
            self.keep_editing,
            self.landing_badge,
            self.landing_title,
            self.landing_body,
            self.landing_note,
        ];
        handles.extend(self.navigation.iter().map(|(_, handle)| *handle));
        handles
    }

    pub(super) fn interactive_handles(&self) -> Vec<HWND> {
        let mut handles = vec![
            self.theme,
            self.extension_mode,
            self.apply,
            self.cancel,
            self.confirm_discard,
            self.keep_editing,
        ];
        handles.extend(self.navigation.iter().map(|(_, handle)| *handle));
        handles
    }
}

#[derive(Debug)]
pub(super) struct ManualControls {
    pub(super) capture: CaptureControls,
    pub(super) video: VideoControls,
    pub(super) audio: AudioControls,
    pub(super) shortcuts: ShortcutControls,
}

impl ManualControls {
    pub(super) fn all_handles(&self) -> Vec<HWND> {
        let mut handles = self.capture.handles.clone();
        handles.extend(self.video.handles.iter().copied());
        handles.extend(self.audio.handles.iter().copied());
        handles.extend(self.shortcuts.handles.iter().copied());
        handles
    }

    pub(super) fn interactive_handles(&self) -> Vec<HWND> {
        let mut handles = vec![
            self.capture.browse,
            self.capture.screenshot_cursor,
            self.video.quality,
            self.video.fps,
            self.video.bitrate,
            self.video.hardware,
            self.video.cursor,
            self.video.duration,
            self.audio.source,
            self.audio.microphone,
            self.audio.refresh,
            self.audio.bitrate,
        ];
        handles.extend(self.shortcuts.hotkeys.iter().map(|(_, _, field)| *field));
        handles
    }

    pub(super) fn page_handles(&self, section: SettingsWindowSection) -> &[HWND] {
        match section {
            SettingsWindowSection::Capture => &self.capture.handles,
            SettingsWindowSection::Video => &self.video.handles,
            SettingsWindowSection::Audio => &self.audio.handles,
            SettingsWindowSection::Shortcuts => &self.shortcuts.handles,
        }
    }
}

#[derive(Debug)]
pub(super) struct CaptureControls {
    pub(super) handles: Vec<HWND>,
    pub(super) title: HWND,
    pub(super) subtitle: HWND,
    pub(super) save_label: HWND,
    pub(super) destination_path: HWND,
    pub(super) browse: HWND,
    pub(super) screenshot_label: HWND,
    pub(super) screenshot_cursor: HWND,
}

#[derive(Debug)]
pub(super) struct VideoControls {
    pub(super) handles: Vec<HWND>,
    pub(super) title: HWND,
    pub(super) subtitle: HWND,
    pub(super) labels: [HWND; 5],
    pub(super) quality: HWND,
    pub(super) fps: HWND,
    pub(super) bitrate: HWND,
    pub(super) hardware: HWND,
    pub(super) cursor: HWND,
    pub(super) duration: HWND,
}

#[derive(Debug)]
pub(super) struct AudioControls {
    pub(super) handles: Vec<HWND>,
    pub(super) title: HWND,
    pub(super) subtitle: HWND,
    pub(super) labels: [HWND; 3],
    pub(super) source: HWND,
    pub(super) microphone: HWND,
    pub(super) refresh: HWND,
    pub(super) bitrate: HWND,
}

#[derive(Debug)]
pub(super) struct ShortcutControls {
    pub(super) handles: Vec<HWND>,
    pub(super) title: HWND,
    pub(super) subtitle: HWND,
    pub(super) hotkeys: Vec<(crate::settings::ManualHotkeyAction, HWND, HWND)>,
}

pub(super) fn create_shell(parent: HWND) -> io::Result<ShellControls> {
    let headline = pages::label(parent, "Manual settings")?;
    let body = pages::label(parent, "Capture locally, even when the browser is closed.")?;
    let theme_label = pages::label(parent, "Theme")?;
    let theme = pages::combo(parent, THEME_ID)?;
    let navigation = SettingsWindowSection::ALL
        .into_iter()
        .enumerate()
        .map(|(index, section)| {
            owner_button(parent, section.label(), NAV_FIRST_ID + index)
                .map(|handle| (section, handle))
        })
        .collect::<io::Result<Vec<_>>>()?;
    Ok(ShellControls {
        headline,
        body,
        theme_label,
        theme,
        navigation,
        extension_mode: owner_button(parent, "Use extension mode", EXTENSION_MODE_ID)?,
        apply: owner_button(parent, "Save changes", APPLY_ID)?,
        cancel: owner_button(parent, "Discard", CANCEL_ID)?,
        feedback: pages::label(parent, "")?,
        confirm_text: pages::label(parent, "Discard unsaved changes?")?,
        confirm_discard: owner_button(parent, "Discard", CONFIRM_DISCARD_ID)?,
        keep_editing: owner_button(parent, "Keep editing", KEEP_EDITING_ID)?,
        landing_badge: pages::label(parent, "EXTENSION MODE")?,
        landing_title: pages::label(parent, "Your browser is in control")?,
        landing_body: pages::label(parent, "")?,
        landing_note: pages::label(parent, "You can switch back at any time.")?,
    })
}

pub(super) fn create_manual(parent: HWND) -> io::Result<ManualControls> {
    Ok(ManualControls {
        capture: pages::create_capture(parent)?,
        video: pages::create_video(parent)?,
        audio: pages::create_audio(parent)?,
        shortcuts: pages::create_shortcuts(parent)?,
    })
}

fn owner_button(parent: HWND, text: &str, id: usize) -> io::Result<HWND> {
    create_control(
        "BUTTON",
        text,
        WS_TABSTOP.0 | u32::try_from(BS_OWNERDRAW).unwrap_or(0),
        parent,
        id,
    )
}
