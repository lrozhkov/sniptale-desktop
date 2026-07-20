//! Typed Manual-mode settings values and drafts.

#[cfg(any(windows, test))]
mod draft;
mod hotkey;
pub(super) mod model;

#[cfg(any(windows, test))]
pub(crate) use draft::{ManualHotkeyUpdateError, ManualProfileDraft, ManualProfileDraftView};
#[cfg(any(windows, test))]
pub(crate) use hotkey::{ManualHotkeyChord, ManualHotkeyKey, parse_manual_hotkey_chord};
#[cfg(any(windows, test))]
pub(crate) use model::{
    ManualAudioSource, ManualFps, ManualHardwarePreference, ManualHotkeyAction, ManualVideoQuality,
};
#[cfg(any(windows, test))]
pub(super) use model::{ManualDestinationSnapshot, ManualRecordingSettings};
pub(super) use model::{ManualProfile, validate_manual_profile};
