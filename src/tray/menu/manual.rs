//! First-party Manual-mode tray menu tree.

use super::{
    actions::{ShellActionId, TrayActionKind},
    mode,
    model::{TrayMenu, TrayMenuEntry, TrayMenuItem},
};
use crate::{settings::OperatingMode, tray::TrayRecordingStatus};

#[must_use]
pub(crate) fn build_manual_menu(recording: TrayRecordingStatus) -> TrayMenu {
    TrayMenu::new(vec![
        TrayMenuEntry::item(
            TrayMenuItem::configured(
                "manual-status",
                "Sniptale — Manual",
                TrayActionKind::OpenSettings,
            )
            .with_enabled(false),
        ),
        TrayMenuEntry::item(
            TrayMenuItem::configured(
                manual_status_id(recording),
                manual_status_label(recording),
                TrayActionKind::OpenSettings,
            )
            .with_enabled(false),
        ),
        TrayMenuEntry::Separator,
        submenu(
            "manual-screenshot",
            "Screenshot",
            screenshot_items(recording),
        ),
        submenu("manual-video", "Video", video_items(recording)),
        TrayMenuEntry::Separator,
        mode::submenu(
            OperatingMode::Manual,
            matches!(recording, TrayRecordingStatus::Idle),
        ),
        TrayMenuEntry::item(item(
            ShellActionId::OpenSettings,
            "Settings…",
            TrayActionKind::OpenSettings,
        )),
        TrayMenuEntry::item(item(
            ShellActionId::OpenSaveFolder,
            "Open save folder",
            TrayActionKind::OpenSaveFolder,
        )),
        TrayMenuEntry::Separator,
        TrayMenuEntry::item(item(
            ShellActionId::Quit,
            "Exit Sniptale",
            TrayActionKind::Quit,
        )),
    ])
}

fn screenshot_items(recording: TrayRecordingStatus) -> Vec<TrayMenuItem> {
    let enabled = matches!(recording, TrayRecordingStatus::Idle);
    vec![
        item(
            ShellActionId::ScreenshotScreen,
            "Current screen",
            TrayActionKind::CaptureScreenshot,
        )
        .with_enabled(enabled),
        item(
            ShellActionId::ScreenshotActiveWindow,
            "Active window",
            TrayActionKind::CaptureScreenshot,
        )
        .with_enabled(enabled),
        item(
            ShellActionId::ScreenshotAllScreens,
            "All screens",
            TrayActionKind::CaptureScreenshot,
        )
        .with_enabled(enabled),
        item(
            ShellActionId::ScreenshotRegion,
            "Region…",
            TrayActionKind::CaptureScreenshot,
        )
        .with_enabled(enabled),
    ]
}

fn video_items(recording: TrayRecordingStatus) -> Vec<TrayMenuItem> {
    let can_start = matches!(recording, TrayRecordingStatus::Idle);
    let mut items = video_start_items(can_start);
    items.extend(video_control_items(recording));
    items
}

fn video_start_items(enabled: bool) -> Vec<TrayMenuItem> {
    vec![
        item(
            ShellActionId::RecordingStartScreen,
            "Start current screen",
            TrayActionKind::StartRecording,
        )
        .with_enabled(enabled),
        item(
            ShellActionId::RecordingStartActiveWindow,
            "Start active window",
            TrayActionKind::StartRecording,
        )
        .with_enabled(enabled),
        item(
            ShellActionId::RecordingStartRegion,
            "Start region…",
            TrayActionKind::StartRecording,
        )
        .with_enabled(enabled),
    ]
}

fn video_control_items(recording: TrayRecordingStatus) -> Vec<TrayMenuItem> {
    let can_pause = matches!(recording, TrayRecordingStatus::Recording);
    let can_resume = matches!(recording, TrayRecordingStatus::Paused);
    let can_stop = matches!(
        recording,
        TrayRecordingStatus::Preparing
            | TrayRecordingStatus::Recording
            | TrayRecordingStatus::Paused
    );
    vec![
        item(
            ShellActionId::RecordingPause,
            "Pause",
            TrayActionKind::PauseRecording,
        )
        .with_enabled(can_pause),
        item(
            ShellActionId::RecordingResume,
            "Resume",
            TrayActionKind::ResumeRecording,
        )
        .with_enabled(can_resume),
        item(
            ShellActionId::RecordingStop,
            "Stop",
            TrayActionKind::StopRecording,
        )
        .with_enabled(can_stop),
        item(
            ShellActionId::RecordingCancel,
            "Cancel and discard",
            TrayActionKind::StopRecording,
        )
        .with_enabled(can_stop),
    ]
}

const fn manual_status_id(status: TrayRecordingStatus) -> &'static str {
    match status {
        TrayRecordingStatus::Idle => "manual-idle",
        TrayRecordingStatus::Preparing => "manual-preparing",
        TrayRecordingStatus::Recording => "manual-recording",
        TrayRecordingStatus::Paused => "manual-paused",
        TrayRecordingStatus::Finalizing => "manual-finalizing",
    }
}

const fn manual_status_label(status: TrayRecordingStatus) -> &'static str {
    match status {
        TrayRecordingStatus::Idle => "Idle",
        TrayRecordingStatus::Preparing => "Preparing",
        TrayRecordingStatus::Recording => "Recording",
        TrayRecordingStatus::Paused => "Paused",
        TrayRecordingStatus::Finalizing => "Finalizing",
    }
}

fn submenu(id: &'static str, label: &'static str, items: Vec<TrayMenuItem>) -> TrayMenuEntry {
    TrayMenuEntry::submenu(
        id,
        label,
        true,
        items.into_iter().map(TrayMenuEntry::item).collect(),
    )
}

fn item(id: ShellActionId, label: &str, kind: TrayActionKind) -> TrayMenuItem {
    TrayMenuItem::configured(id.as_str(), label, kind).with_offline_capable(true)
}
