//! Manual page child-control construction.

use super::*;
use windows::Win32::{
    System::SystemServices::{SS_NOPREFIX, SS_PATHELLIPSIS},
    UI::WindowsAndMessaging::{
        BS_AUTOCHECKBOX, BS_OWNERDRAW, CBS_DROPDOWNLIST, CBS_HASSTRINGS, CBS_OWNERDRAWFIXED,
        WS_TABSTOP, WS_VSCROLL,
    },
};

pub(super) fn create_capture(parent: HWND) -> io::Result<CaptureControls> {
    let title = label(parent, "Capture")?;
    let subtitle = label(
        parent,
        "Choose where files are saved and how screenshots look.",
    )?;
    let save_label = label(parent, "Save location")?;
    let destination_path = path_label(parent, "Choose a folder")?;
    let browse = owner_button(parent, "Change folder…", BROWSE_DESTINATION_ID)?;
    let screenshot_label = label(parent, "Screenshots")?;
    let screenshot_cursor = checkbox(
        parent,
        "Include pointer in screenshots",
        SCREENSHOT_CURSOR_ID,
    )?;
    Ok(CaptureControls {
        handles: vec![
            title,
            subtitle,
            save_label,
            destination_path,
            browse,
            screenshot_label,
            screenshot_cursor,
        ],
        title,
        subtitle,
        save_label,
        destination_path,
        browse,
        screenshot_label,
        screenshot_cursor,
    })
}

pub(super) fn create_video(parent: HWND) -> io::Result<VideoControls> {
    let title = label(parent, "Video")?;
    let subtitle = label(parent, "Tune recording quality, performance, and duration.")?;
    let labels = [
        label(parent, "Quality")?,
        label(parent, "Frame rate")?,
        label(parent, "Video bitrate")?,
        label(parent, "Encoder")?,
        label(parent, "Maximum duration")?,
    ];
    let quality = combo(parent, VIDEO_QUALITY_ID)?;
    let fps = combo(parent, VIDEO_FPS_ID)?;
    let bitrate = combo(parent, VIDEO_BITRATE_ID)?;
    let hardware = combo(parent, VIDEO_HARDWARE_ID)?;
    let duration = combo(parent, VIDEO_DURATION_ID)?;
    let cursor = checkbox(parent, "Include pointer in video", VIDEO_CURSOR_ID)?;
    let mut handles = vec![title, subtitle];
    handles.extend(labels);
    handles.extend([quality, fps, bitrate, hardware, duration, cursor]);
    Ok(VideoControls {
        handles,
        title,
        subtitle,
        labels,
        quality,
        fps,
        bitrate,
        hardware,
        cursor,
        duration,
    })
}

pub(super) fn create_audio(parent: HWND) -> io::Result<AudioControls> {
    let title = label(parent, "Audio")?;
    let subtitle = label(
        parent,
        "Choose what is recorded and which microphone is used.",
    )?;
    let labels = [
        label(parent, "Source")?,
        label(parent, "Microphone")?,
        label(parent, "Audio quality")?,
    ];
    let source = combo(parent, AUDIO_SOURCE_ID)?;
    let microphone = combo(parent, MICROPHONE_DEVICE_ID)?;
    let refresh = owner_button(parent, "Refresh devices", REFRESH_MICROPHONES_ID)?;
    let bitrate = combo(parent, AUDIO_BITRATE_ID)?;
    let mut handles = vec![title, subtitle];
    handles.extend(labels);
    handles.extend([source, microphone, refresh, bitrate]);
    Ok(AudioControls {
        handles,
        title,
        subtitle,
        labels,
        source,
        microphone,
        refresh,
        bitrate,
    })
}

pub(super) fn create_shortcuts(parent: HWND) -> io::Result<ShortcutControls> {
    let title = label(parent, "Shortcuts")?;
    let subtitle = label(
        parent,
        "Click a field, then press the shortcut you want to use.",
    )?;
    let mut handles = vec![title, subtitle];
    let mut hotkeys = Vec::new();
    for (index, action) in crate::settings::ManualHotkeyAction::ALL
        .into_iter()
        .enumerate()
    {
        let action_label = label(parent, hotkey_label(action))?;
        let field = owner_button(parent, "Not set", HOTKEY_FIRST_ID + index)?;
        handles.extend([action_label, field]);
        hotkeys.push((action, action_label, field));
    }
    Ok(ShortcutControls {
        handles,
        title,
        subtitle,
        hotkeys,
    })
}

const fn hotkey_label(action: crate::settings::ManualHotkeyAction) -> &'static str {
    use crate::settings::ManualHotkeyAction;
    match action {
        ManualHotkeyAction::ScreenshotScreen => "Capture screen",
        ManualHotkeyAction::ScreenshotActiveWindow => "Capture active window",
        ManualHotkeyAction::ScreenshotRegion => "Capture region",
        ManualHotkeyAction::RecordingStartScreen => "Record screen",
        ManualHotkeyAction::RecordingStartActiveWindow => "Record active window",
        ManualHotkeyAction::RecordingStartRegion => "Record region",
        ManualHotkeyAction::RecordingPauseResume => "Pause / resume recording",
        ManualHotkeyAction::RecordingStop => "Stop recording",
    }
}

pub(super) fn label(parent: HWND, text: &str) -> io::Result<HWND> {
    create_control("STATIC", text, SS_NOPREFIX.0, parent, 0)
}

fn path_label(parent: HWND, text: &str) -> io::Result<HWND> {
    create_control("STATIC", text, SS_NOPREFIX.0 | SS_PATHELLIPSIS.0, parent, 0)
}

pub(super) fn combo(parent: HWND, id: usize) -> io::Result<HWND> {
    create_control(
        "COMBOBOX",
        "",
        WS_TABSTOP.0
            | WS_VSCROLL.0
            | u32::try_from(CBS_DROPDOWNLIST).unwrap_or(0)
            | u32::try_from(CBS_HASSTRINGS).unwrap_or(0)
            | u32::try_from(CBS_OWNERDRAWFIXED).unwrap_or(0),
        parent,
        id,
    )
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

fn checkbox(parent: HWND, text: &str, id: usize) -> io::Result<HWND> {
    create_control(
        "BUTTON",
        text,
        WS_TABSTOP.0 | u32::try_from(BS_AUTOCHECKBOX).unwrap_or(0),
        parent,
        id,
    )
}
