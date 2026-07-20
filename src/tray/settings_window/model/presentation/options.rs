//! Exact typed options for numeric Manual fields with custom persisted values.

use super::SettingsWindowPresentation;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SettingsVideoBitrateOption {
    value: Option<u16>,
    label: String,
}

impl SettingsVideoBitrateOption {
    #[must_use]
    pub(crate) const fn value(&self) -> Option<u16> {
        self.value
    }

    #[must_use]
    pub(crate) fn label(&self) -> &str {
        &self.label
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SettingsVideoDurationOption {
    value: u32,
    label: String,
}

impl SettingsVideoDurationOption {
    #[must_use]
    pub(crate) const fn value(&self) -> u32 {
        self.value
    }

    #[must_use]
    pub(crate) fn label(&self) -> &str {
        &self.label
    }
}

impl SettingsWindowPresentation {
    #[must_use]
    pub(crate) fn video_bitrate_options(&self) -> Vec<SettingsVideoBitrateOption> {
        let current = self
            .manual
            .as_ref()
            .and_then(|manual| manual.video_bitrate_override_mbps);
        let mut values = vec![None, Some(8), Some(16), Some(32), Some(48)];
        if current.is_some() && !values.contains(&current) {
            values.push(current);
            values.sort_unstable();
        }
        values
            .into_iter()
            .map(|value| SettingsVideoBitrateOption {
                value,
                label: value
                    .map_or_else(|| "Automatic".to_owned(), |value| format!("{value} Mbps")),
            })
            .collect()
    }

    #[must_use]
    pub(crate) fn video_duration_options(&self) -> Vec<SettingsVideoDurationOption> {
        let current = self
            .manual
            .as_ref()
            .map_or(3_600, |manual| manual.video_max_duration_seconds);
        let mut values = vec![60, 300, 900, 3_600, 10_800];
        if !values.contains(&current) {
            values.push(current);
            values.sort_unstable();
        }
        values
            .into_iter()
            .map(|value| SettingsVideoDurationOption {
                value,
                label: duration_label(value),
            })
            .collect()
    }
}

fn duration_label(seconds: u32) -> String {
    if seconds.is_multiple_of(3_600) {
        let hours = seconds / 3_600;
        format!("{hours} {}", if hours == 1 { "hour" } else { "hours" })
    } else if seconds.is_multiple_of(60) {
        let minutes = seconds / 60;
        format!(
            "{minutes} {}",
            if minutes == 1 { "minute" } else { "minutes" }
        )
    } else {
        format!(
            "{seconds} {}",
            if seconds == 1 { "second" } else { "seconds" }
        )
    }
}
