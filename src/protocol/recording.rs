use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as DeError};
use serde_json::{Number, Value};

use super::capabilities::VideoMode;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeRecordingRegion {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeRecordingSource {
    pub(crate) mode: VideoMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) display_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) window_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) region: Option<NativeRecordingRegion>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) microphone_device_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeAudioSourceMode {
    Microphone,
    System,
    Mixed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum NativeVideoFrameRate {
    Auto,
    Fps24,
    Fps30,
    Fps60,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub(crate) enum NativeVideoQuality {
    Low,
    Medium,
    High,
    Ultra,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeRequestedQualitySettings {
    pub(crate) audio_bitrate_kbps: u16,
    pub(crate) audio_source_mode: NativeAudioSourceMode,
    pub(crate) frame_rate: NativeVideoFrameRate,
    pub(crate) quality: NativeVideoQuality,
    pub(crate) video_bitrate_mbps_override: Option<Number>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum NativeEffectiveEncoder {
    Hardware,
    Software,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeEffectiveQualitySettings {
    pub(crate) audio_bitrate_kbps: u16,
    pub(crate) audio_source_mode: NativeAudioSourceMode,
    pub(crate) frame_rate: NativeVideoFrameRate,
    pub(crate) quality: NativeVideoQuality,
    pub(crate) video_bitrate_mbps_override: Option<Number>,
    pub(crate) effective_audio_bitrate_kbps: Number,
    pub(crate) effective_fps: Number,
    pub(crate) effective_video_bitrate_mbps: Number,
    pub(crate) encoder: NativeEffectiveEncoder,
    pub(crate) warnings: Vec<NativeSettingsWarning>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeSettingsWarning {
    pub(crate) code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) message: Option<String>,
}

impl Serialize for NativeVideoFrameRate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Auto => serializer.serialize_str("auto"),
            Self::Fps24 => serializer.serialize_u16(24),
            Self::Fps30 => serializer.serialize_u16(30),
            Self::Fps60 => serializer.serialize_u16(60),
        }
    }
}

impl<'de> Deserialize<'de> for NativeVideoFrameRate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match Value::deserialize(deserializer)? {
            Value::String(value) if value == "auto" => Ok(Self::Auto),
            Value::Number(value) if value.as_u64() == Some(24) => Ok(Self::Fps24),
            Value::Number(value) if value.as_u64() == Some(30) => Ok(Self::Fps30),
            Value::Number(value) if value.as_u64() == Some(60) => Ok(Self::Fps60),
            _ => Err(D::Error::custom("frameRate must be auto, 24, 30, or 60")),
        }
    }
}
