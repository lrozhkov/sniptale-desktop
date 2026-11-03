//! Exact typed options for numeric Manual fields with custom persisted values.

use super::SettingsWindowPresentation;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SettingsVideoBitrateOption {
    value: Option<u16>,
}

impl SettingsVideoBitrateOption {
    #[must_use]
    pub(crate) const fn value(&self) -> Option<u16> {
        self.value
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SettingsVideoDurationOption {
    value: u32,
}

impl SettingsVideoDurationOption {
    #[must_use]
    pub(crate) const fn value(&self) -> u32 {
        self.value
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
            .map(|value| SettingsVideoBitrateOption { value })
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
            .map(|value| SettingsVideoDurationOption { value })
            .collect()
    }
}
