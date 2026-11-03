//! Settings presentation messages. Profile validation remains in settings.

use crate::settings::ManualProfileDraftView;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "command", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum SettingsRequest {
    Open {},
    ChooseFolder {},
    RefreshDevices {},
    RefreshBrowsers {},
    SaveDelivery {
        #[serde(rename = "targetId", deserialize_with = "required_selection")]
        target_id: Option<String>,
        #[serde(rename = "askAfterCapture")]
        ask_after_capture: bool,
    },
    Apply {
        #[serde(rename = "expectedRevision")]
        expected_revision: u64,
        draft: ManualProfileDraftView,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SettingsSnapshot {
    pub(crate) profile_revision: u64,
    pub(crate) selected_folder: Option<String>,
    pub(crate) draft: ManualProfileDraftView,
    pub(crate) microphones: Vec<MicrophoneOption>,
    pub(crate) audio_sources: Vec<AudioSourceOption>,
    pub(crate) delivery: BrowserDelivery,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct MicrophoneOption {
    pub(crate) id: Option<String>,
    pub(crate) name: String,
    pub(crate) missing: bool,
    pub(crate) available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct AudioSourceOption {
    pub(crate) source: crate::settings::ManualAudioSource,
    pub(crate) available: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BrowserDelivery {
    pub(crate) targets: Vec<BrowserDestination>,
    pub(crate) target_id: Option<String>,
    pub(crate) ask_after_capture: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct BrowserDestination {
    pub(crate) id: String,
    pub(crate) label: String,
}

fn required_selection<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<String>, D::Error> {
    Option::<String>::deserialize(deserializer)
}

#[cfg(test)]
#[path = "settings/tests.rs"]
mod tests;
