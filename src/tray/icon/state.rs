//! Shell notification icon owner.

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrayStartupStatus {
    Operational,
    RepairRequired,
    UnsupportedCacheSchema,
    RepairRequiredAndUnsupportedCacheSchema,
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrayConnectionStatus {
    Connected,
    Disconnected,
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrayRecordingStatus {
    Idle,
    Preparing,
    Recording,
    Paused,
    Finalizing,
}

/// A semantic icon choice. The Windows shell adapter owns the resource handles.
#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrayIconVariant {
    Normal,
    Recording,
    Paused,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrayNoticeKind {
    ActionRejected,
    #[cfg(any(windows, test))]
    ExtensionDisconnected,
}

#[cfg(any(windows, test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TrayIconState {
    pub(crate) connection: TrayConnectionStatus,
    pub(crate) recording: TrayRecordingStatus,
    pub(crate) startup_status: TrayStartupStatus,
}

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TrayIconCommand {
    Add { tooltip: String },
    Update { tooltip: String },
    Notice { kind: TrayNoticeKind },
    Remove,
}

#[cfg(windows)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TrayNotice {
    pub(crate) title: &'static str,
    pub(crate) body: &'static str,
}

#[cfg(test)]
pub(crate) fn plan_icon_update(
    previous: Option<&TrayIconState>,
    next: &TrayIconState,
    notice: Option<TrayNoticeKind>,
) -> Vec<TrayIconCommand> {
    let mut commands = Vec::new();
    let tooltip = tooltip(next);
    match previous {
        None => commands.push(TrayIconCommand::Add { tooltip }),
        Some(previous) if previous != next => commands.push(TrayIconCommand::Update { tooltip }),
        Some(_) => {}
    }
    if let Some(kind) = notice {
        commands.push(TrayIconCommand::Notice { kind });
    }
    commands
}

#[cfg(any(windows, test))]
pub(crate) fn tray_icon_tooltip(state: &TrayIconState) -> String {
    tooltip(state)
}

#[cfg(any(windows, test))]
pub(crate) fn manual_tray_icon_tooltip(recording: TrayRecordingStatus) -> String {
    let recording = recording_label(recording);
    format!("Sniptale: Manual, {recording}")
}

/// Keeps recording indication independent from the Win32 resource implementation.
#[must_use]
#[cfg(any(windows, test))]
pub(crate) const fn tray_icon_variant(state: &TrayIconState) -> TrayIconVariant {
    match state.recording {
        TrayRecordingStatus::Idle => TrayIconVariant::Normal,
        TrayRecordingStatus::Paused => TrayIconVariant::Paused,
        TrayRecordingStatus::Preparing
        | TrayRecordingStatus::Recording
        | TrayRecordingStatus::Finalizing => TrayIconVariant::Recording,
    }
}

#[cfg(test)]
pub(crate) fn plan_icon_remove(installed: bool) -> Vec<TrayIconCommand> {
    if installed {
        vec![TrayIconCommand::Remove]
    } else {
        Vec::new()
    }
}

#[cfg(any(windows, test))]
fn tooltip(state: &TrayIconState) -> String {
    let connection = match state.connection {
        TrayConnectionStatus::Connected => "connected",
        TrayConnectionStatus::Disconnected => "disconnected",
    };
    let recording = recording_label(state.recording);
    let startup = match state.startup_status {
        TrayStartupStatus::Operational => return format!("Sniptale: {connection}, {recording}"),
        TrayStartupStatus::RepairRequired => "repair required",
        TrayStartupStatus::UnsupportedCacheSchema => "unsupported cache schema",
        TrayStartupStatus::RepairRequiredAndUnsupportedCacheSchema => {
            "repair required, unsupported cache schema"
        }
    };
    format!("Sniptale: {connection}, {recording}, {startup}")
}

#[cfg(any(windows, test))]
const fn recording_label(recording: TrayRecordingStatus) -> &'static str {
    match recording {
        TrayRecordingStatus::Idle => "idle",
        TrayRecordingStatus::Preparing => "preparing",
        TrayRecordingStatus::Recording => "recording",
        TrayRecordingStatus::Paused => "paused",
        TrayRecordingStatus::Finalizing => "finalizing",
    }
}

#[cfg(windows)]
pub(crate) const fn notice_content(kind: TrayNoticeKind) -> TrayNotice {
    match kind {
        TrayNoticeKind::ActionRejected => TrayNotice {
            title: "Sniptale",
            body: "The requested tray action could not be completed.",
        },
        TrayNoticeKind::ExtensionDisconnected => TrayNotice {
            title: "Sniptale",
            body: "Connect the browser extension to use this action.",
        },
    }
}
