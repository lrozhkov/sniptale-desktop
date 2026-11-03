//! Typed Manual-mode settings values and drafts.

#[cfg(any(windows, test))]
mod draft;
mod hotkey;
pub(super) mod model;

#[cfg(test)]
pub(crate) use draft::ManualHotkeyUpdateError;
#[cfg(any(windows, test))]
pub(crate) use draft::{ManualProfileDraft, ManualProfileDraftView};
#[cfg(any(windows, test))]
pub(crate) use hotkey::{ManualHotkeyChord, ManualHotkeyKey, parse_manual_hotkey_chord};
#[cfg(any(windows, test))]
pub(crate) use model::{ManualAudioSource, ManualHardwarePreference, ManualHotkeyAction};
#[cfg(any(windows, test))]
pub(super) use model::{ManualDestinationSnapshot, ManualRecordingSettings};
#[cfg(test)]
pub(crate) use model::{ManualFps, ManualVideoQuality};
pub(super) use model::{ManualProfile, validate_manual_profile};
