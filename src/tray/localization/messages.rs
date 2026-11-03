//! Typed message identifiers and arguments keep translated text out of application logic.

use fluent_bundle::FluentArgs;

macro_rules! messages {
    ($($variant:ident => $id:literal),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub(crate) enum Text { $($variant),+ }
        impl Text {
            #[cfg(test)]
            pub(crate) const ALL: &'static [Self] = &[$(Self::$variant),+];
            pub(crate) const fn id(self) -> &'static str {
                match self { $(Self::$variant => $id),+ }
            }
        }
    };
}

messages! {
    SandboxHeading => "sandbox-heading",
    SandboxDescription => "sandbox-description",
    ThemeLabel => "sandbox-theme-label",
    LanguageLabel => "sandbox-language-label",
    ThemeLight => "theme-light",
    ThemeDark => "theme-dark",
    ThemeContrast => "theme-contrast",
    ThemeChangeFailed => "theme-change-failed",
    LanguageSystem => "language-system",
    Capture => "navigation-capture",
    Video => "navigation-video",
    CaptureFixture => "command-capture-fixture",
    SecondaryCommand => "command-secondary",
    ResetCounter => "command-reset-counter",
    CommandsHeading => "commands-heading",
    ValuesHeading => "values-heading",
    IncludeCursor => "field-include-cursor",
    PendingFixture => "field-pending-fixture",
    SaveFolderFixture => "field-save-folder-fixture",
    HotkeySpecimen => "field-hotkey-specimen",
    Ready => "feedback-ready",
    CounterReset => "feedback-counter-reset",
    CaptureSelected => "feedback-capture-selected",
    VideoSelected => "feedback-video-selected",
    ShortcutPress => "shortcut-press",
    ShortcutUnset => "shortcut-unset",
    ShortcutNeedsModifier => "shortcut-needs-modifier",
    ShortcutInvalid => "shortcut-invalid",
    ShortcutConflict => "shortcut-conflict",
    ShortcutAccepted => "shortcut-accepted",
    ShortcutCancelled => "shortcut-cancelled",
    ShortcutCleared => "shortcut-cleared",
    PanelNeedsDestination => "panel-needs-destination",
    PanelSetup => "panel-setup",
    PanelTitle => "panel-title",
    PanelScreenshotDone => "panel-screenshot-done",
    PanelSaved => "panel-saved",
    PanelDiscarded => "panel-discarded",
    PanelAllScreens => "panel-all-screens",
    PanelBusy => "panel-busy",
    PanelCapture => "panel-capture",
    PanelDiscard => "panel-discard",
    PanelFinalizing => "panel-finalizing",
    PanelMenu => "panel-menu",
    PanelPause => "panel-pause",
    PanelPaused => "panel-paused",
    PanelPreparing => "panel-preparing",
    PanelReady => "panel-ready",
    PanelRecord => "panel-record",
    PanelRecording => "panel-recording",
    PanelRegion => "panel-region",
    PanelResume => "panel-resume",
    PanelScreen => "panel-screen",
    PanelScreenshot => "panel-screenshot",
    PanelSelectRegion => "panel-select-region",
    PanelNoWindow => "panel-no-window",
    PanelSelectWindow => "panel-select-window",
    PanelSettings => "panel-settings",
    PanelStop => "panel-stop",
    PanelTargetLabel => "panel-target-label",
    PanelUnavailable => "panel-unavailable",
    PanelVideo => "panel-video",
    PanelWindow => "panel-window",
    SceneLabel => "sandbox-scene-label",
    SceneComponents => "sandbox-scene-components",
    SceneCapturePanel => "sandbox-scene-capture-panel",
    EncoderPrefer => "encoder-prefer",
    EncoderSoftware => "encoder-software",
    FeedbackAppearanceFailed => "feedback-appearance-failed",
    FeedbackAudioUnavailable => "feedback-audio-unavailable",
    FeedbackHotkeyUnavailable => "feedback-hotkey-unavailable",
    FeedbackMicrophoneUnavailable => "feedback-microphone-unavailable",
    FeedbackRecordingBusy => "feedback-recording-busy",
    FeedbackSaveFailed => "feedback-save-failed",
    OptionAutomatic => "option-automatic",
    QualityBalanced => "quality-balanced",
    QualityHigh => "quality-high",
    QualityLow => "quality-low",
    QualityUltra => "quality-ultra",
    SettingsApply => "settings-apply",
    SettingsApplyExtension => "settings-apply-extension",
    SettingsApplyManual => "settings-apply-manual",
    SettingsAskDestination => "settings-ask-destination",
    SettingsAudio => "settings-audio",
    SettingsAudioDescription => "settings-audio-description",
    SettingsAudioMicrophone => "settings-audio-microphone",
    SettingsAudioMixed => "settings-audio-mixed",
    SettingsAudioQuality => "settings-audio-quality",
    SettingsAudioSource => "settings-audio-source",
    SettingsAudioSystem => "settings-audio-system",
    SettingsCancel => "settings-cancel",
    SettingsCapture => "settings-capture",
    SettingsChangeFolder => "settings-change-folder",
    SettingsChooseLocalFolder => "settings-choose-local-folder",
    SettingsDelivery => "settings-delivery",
    SettingsDescription => "settings-description",
    SettingsDeviceDefault => "settings-device-default",
    SettingsDeviceMissing => "settings-device-missing",
    SettingsDirtyDescription => "settings-dirty-description",
    SettingsDiscard => "settings-discard",
    SettingsDuration => "settings-duration",
    SettingsEncoder => "settings-encoder",
    SettingsExtensionDescription => "settings-extension-description",
    SettingsExtensionNote => "settings-extension-note",
    SettingsExtensionTitle => "settings-extension-title",
    SettingsFrameRate => "settings-frame-rate",
    SettingsHeading => "settings-heading",
    SettingsKeepEditing => "settings-keep-editing",
    SettingsLocalDescription => "settings-local-description",
    SettingsLocalFolder => "settings-local-folder",
    SettingsMicrophone => "settings-microphone",
    SettingsPointerHighlights => "settings-pointer-highlights",
    SettingsQuality => "settings-quality",
    SettingsRefreshBrowsers => "settings-refresh-browsers",
    SettingsRefreshDevices => "settings-refresh-devices",
    SettingsSaveDelivery => "settings-save-delivery",
    SettingsSaveLocation => "settings-save-location",
    SettingsScreenshotCursor => "settings-screenshot-cursor",
    SettingsScreenshots => "settings-screenshots",
    SettingsSetupDescription => "settings-setup-description",
    SettingsSetupNote => "settings-setup-note",
    SettingsSetupTitle => "settings-setup-title",
    SettingsShortcuts => "settings-shortcuts",
    SettingsShortcutsDescription => "settings-shortcuts-description",
    SettingsSystemTheme => "settings-system-theme",
    SettingsTitle => "settings-title",
    SettingsTheme => "settings-theme",
    SettingsUseExtension => "settings-use-extension",
    SettingsVideo => "settings-video",
    SettingsVideoBitrate => "settings-video-bitrate",
    SettingsVideoCursor => "settings-video-cursor",
    SettingsVideoDescription => "settings-video-description",
    ShortcutCapturing => "shortcut-capturing",
    ShortcutNotSet => "shortcut-not-set",
    ShortcutPauseResume => "shortcut-pause-resume",
    ShortcutRecordRegion => "shortcut-record-region",
    ShortcutRecordScreen => "shortcut-record-screen",
    ShortcutRecordWindow => "shortcut-record-window",
    ShortcutRegion => "shortcut-region",
    ShortcutScreen => "shortcut-screen",
    ShortcutStop => "shortcut-stop",
    ShortcutWindow => "shortcut-window",
    SettingsChooseFolder => "settings-choose-folder",
    MenuMode => "menu-mode",
    MenuLocal => "menu-local",
    MenuBrowser => "menu-browser",
    MenuUnavailable => "menu-unavailable",
    MenuSelected => "menu-selected",
    MenuCancelled => "menu-cancelled",
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum FormattedText<'a> {
    SandboxTitle {
        version: &'a str,
    },
    SandboxFooter {
        version: &'a str,
        locale: &'a str,
    },
    ActivationCount(u32),
    SettingsValue {
        value: u32,
        unit: ValueUnit,
    },
    MicrophoneName {
        name: &'a str,
        is_default: bool,
        available: bool,
    },
    UnavailableChoice {
        label: &'a str,
    },
    RecordingTime {
        state: &'a str,
        time: &'a str,
    },
    ShortcutConflict {
        action: &'a str,
    },
}

impl<'a> FormattedText<'a> {
    pub(super) fn parts(self) -> (&'static str, FluentArgs<'a>) {
        let mut args = FluentArgs::new();
        let id = match self {
            Self::SandboxTitle { version } => {
                args.set("version", version);
                "sandbox-title"
            }
            Self::SandboxFooter { version, locale } => {
                args.set("version", version);
                args.set("locale", locale);
                "sandbox-footer"
            }
            Self::SettingsValue { value, unit } => {
                args.set("value", i64::from(value));
                args.set("count", i64::from(value));
                match unit {
                    ValueUnit::Mbps => "value-mbps",
                    ValueUnit::Kbps => "value-kbps",
                    ValueUnit::Fps => "value-fps",
                    ValueUnit::Hours => "value-hours",
                    ValueUnit::Minutes => "value-minutes",
                    ValueUnit::Seconds => "value-seconds",
                }
            }
            Self::MicrophoneName {
                name,
                is_default,
                available,
            } => {
                args.set("name", name);
                args.set(
                    "status",
                    match (is_default, available) {
                        (true, true) => "default",
                        (true, false) => "default-unavailable",
                        (false, true) => "available",
                        (false, false) => "unavailable",
                    },
                );
                "microphone-name"
            }
            Self::UnavailableChoice { label } => {
                args.set("label", label);
                "choice-unavailable"
            }
            Self::RecordingTime { state, time } => {
                args.set("state", state);
                args.set("time", time);
                "panel-recording-time"
            }
            Self::ShortcutConflict { action } => {
                args.set("action", action);
                "feedback-shortcut-conflict"
            }
            Self::ActivationCount(count) => {
                args.set("count", i64::from(count));
                "feedback-activation-count"
            }
        };
        (id, args)
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) enum ValueUnit {
    Mbps,
    Kbps,
    Fps,
    Hours,
    Minutes,
    Seconds,
}
