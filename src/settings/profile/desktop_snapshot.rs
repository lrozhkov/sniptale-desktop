//! Read-only browser projection of the Desktop-owned local capture preferences.

use super::super::{LocalProfile, ManualProfileDraft};
use crate::protocol::{
    NativeAudioCodecSetting, NativeContainerSetting, NativeHardwareAcceleration,
    NativeScreenshotSettings, NativeSettingsSnapshot, NativeTrayActionRegistry,
    NativeVideoAdvancedSettings, NativeVideoCodecSetting, NativeVideoCodecSettings,
    NativeVideoSettings, NativeVideoTelemetrySettings,
};

pub(crate) fn desktop_settings_snapshot(profile: &LocalProfile) -> NativeSettingsSnapshot {
    let manual = profile.manual_profile.clone().unwrap_or_else(|| {
        ManualProfileDraft::with_defaults(String::new(), String::new()).into_profile()
    });
    let preferences = manual.recording_settings();
    NativeSettingsSnapshot {
        screenshots: NativeScreenshotSettings {
            include_cursor: manual.screenshot.include_cursor,
        },
        video: NativeVideoSettings {
            enabled: profile.manual_profile.is_some(),
            advanced: NativeVideoAdvancedSettings {
                audio_bitrate_kbps: preferences.requested_quality.audio_bitrate_kbps,
                audio_source_mode: preferences.requested_quality.audio_source_mode,
                frame_rate: preferences.requested_quality.frame_rate,
                include_cursor_in_video: preferences.include_cursor,
                max_duration_minutes: serde_json::Number::from(
                    manual.video.max_duration_seconds.div_ceil(60),
                ),
                prefer_hardware_encoder: manual.video.hardware_preference
                    == super::super::ManualHardwarePreference::Prefer,
                video_bitrate_mbps_override: preferences
                    .requested_quality
                    .video_bitrate_mbps_override,
            },
            codec: desktop_codec(manual.video.hardware_preference),
            telemetry: NativeVideoTelemetrySettings {
                collect_cursor: manual.video.collect_pointer_events,
                collect_clicks: manual.video.collect_pointer_events,
                collect_key_events: false,
                collect_typing_spans: false,
                collect_static_signals: false,
            },
        },
        tray_actions: NativeTrayActionRegistry {
            actions: Vec::new(),
            revision: profile.revision().to_string(),
            shortcut_priority: None,
        },
    }
}

fn desktop_codec(preference: super::super::ManualHardwarePreference) -> NativeVideoCodecSettings {
    NativeVideoCodecSettings {
        audio_codec: NativeAudioCodecSetting::Aac,
        container: NativeContainerSetting::Mp4,
        hardware_acceleration: match preference {
            super::super::ManualHardwarePreference::Prefer => NativeHardwareAcceleration::Prefer,
            super::super::ManualHardwarePreference::ForceSoftware => {
                NativeHardwareAcceleration::ForceSoftware
            }
        },
        video_codec: NativeVideoCodecSetting::H264,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unconfigured_desktop_snapshot_disables_recording_and_exports_no_destination()
    -> Result<(), Box<dyn std::error::Error>> {
        let profile =
            crate::settings::ensure_local_profile_from_path(None).map_err(|_| "profile")?;
        let snapshot = desktop_settings_snapshot(&profile);
        assert!(!snapshot.video.enabled);
        let serialized = serde_json::to_string(&snapshot)?;
        assert!(!serialized.contains("destination"));
        assert_eq!(snapshot.tray_actions.actions.len(), 0);
        Ok(())
    }
}
