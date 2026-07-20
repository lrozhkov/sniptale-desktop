//! Deterministic shortcut-recorder state independent from Win32 virtual keys.

use crate::settings::{ManualHotkeyAction, ManualHotkeyChord, ManualHotkeyKey};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CaptureKey {
    Modifier(u8),
    Character(char),
    Function(u8),
    PrintScreen,
    Escape,
    Delete,
    Backspace,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CaptureOutcome {
    Ignored,
    Preview(String),
    Commit {
        action: ManualHotkeyAction,
        chord: ManualHotkeyChord,
    },
    Clear(ManualHotkeyAction),
    Cancel(ManualHotkeyAction),
    Invalid,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct HotkeyCaptureState {
    action: Option<ManualHotkeyAction>,
    modifiers: u8,
}

impl HotkeyCaptureState {
    pub(crate) fn begin(&mut self, action: ManualHotkeyAction) {
        self.action = Some(action);
        self.modifiers = 0;
    }

    #[must_use]
    pub(crate) const fn active_action(self) -> Option<ManualHotkeyAction> {
        self.action
    }

    pub(crate) fn key_down(&mut self, key: CaptureKey) -> CaptureOutcome {
        let Some(action) = self.action else {
            return CaptureOutcome::Ignored;
        };
        match key {
            CaptureKey::Modifier(modifier) => {
                self.modifiers |= modifier;
                CaptureOutcome::Preview(modifier_preview(self.modifiers))
            }
            CaptureKey::Escape => {
                self.reset();
                CaptureOutcome::Cancel(action)
            }
            CaptureKey::Delete | CaptureKey::Backspace => {
                self.reset();
                CaptureOutcome::Clear(action)
            }
            CaptureKey::Unsupported => CaptureOutcome::Invalid,
            CaptureKey::Character(character) => self.commit(
                action,
                ManualHotkeyKey::Character(character.to_ascii_uppercase()),
            ),
            CaptureKey::Function(index) => self.commit(action, ManualHotkeyKey::Function(index)),
            CaptureKey::PrintScreen => self.commit(action, ManualHotkeyKey::PrintScreen),
        }
    }

    pub(crate) fn modifier_up(&mut self, modifier: u8) {
        self.modifiers &= !modifier;
    }

    pub(crate) fn cancel(&mut self) -> Option<ManualHotkeyAction> {
        let action = self.action;
        self.reset();
        action
    }

    fn commit(&mut self, action: ManualHotkeyAction, key: ManualHotkeyKey) -> CaptureOutcome {
        let Some(chord) = ManualHotkeyChord::new(key, self.modifiers) else {
            return CaptureOutcome::Invalid;
        };
        self.reset();
        CaptureOutcome::Commit { action, chord }
    }

    fn reset(&mut self) {
        self.action = None;
        self.modifiers = 0;
    }
}

fn modifier_preview(modifiers: u8) -> String {
    let mut labels = Vec::new();
    for (modifier, label) in [
        (ManualHotkeyChord::CTRL, "Ctrl"),
        (ManualHotkeyChord::SHIFT, "Shift"),
        (ManualHotkeyChord::ALT, "Alt"),
        (ManualHotkeyChord::META, "Win"),
    ] {
        if modifiers & modifier != 0 {
            labels.push(label);
        }
    }
    format!("{} + …", labels.join(" + "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorder_commits_supported_shortcut_and_clears_state() {
        let mut state = HotkeyCaptureState::default();
        state.begin(ManualHotkeyAction::ScreenshotScreen);
        assert_eq!(
            state.key_down(CaptureKey::Modifier(ManualHotkeyChord::CTRL)),
            CaptureOutcome::Preview("Ctrl + …".to_owned())
        );
        let outcome = state.key_down(CaptureKey::Character('s'));
        assert!(matches!(outcome, CaptureOutcome::Commit { .. }));
        assert_eq!(state.active_action(), None);
    }

    #[test]
    fn recorder_rejects_unmodified_character_but_accepts_print_screen() {
        let mut state = HotkeyCaptureState::default();
        state.begin(ManualHotkeyAction::ScreenshotScreen);
        assert_eq!(
            state.key_down(CaptureKey::Character('S')),
            CaptureOutcome::Invalid
        );
        assert!(state.active_action().is_some());
        assert!(matches!(
            state.key_down(CaptureKey::PrintScreen),
            CaptureOutcome::Commit { .. }
        ));
    }

    #[test]
    fn escape_cancels_and_delete_clears() {
        let mut state = HotkeyCaptureState::default();
        state.begin(ManualHotkeyAction::RecordingStop);
        assert_eq!(
            state.key_down(CaptureKey::Escape),
            CaptureOutcome::Cancel(ManualHotkeyAction::RecordingStop)
        );
        state.begin(ManualHotkeyAction::RecordingStop);
        assert_eq!(
            state.key_down(CaptureKey::Delete),
            CaptureOutcome::Clear(ManualHotkeyAction::RecordingStop)
        );
    }

    #[test]
    fn function_backspace_and_modifier_release_follow_recorder_rules() {
        let mut state = HotkeyCaptureState::default();
        state.begin(ManualHotkeyAction::RecordingStop);
        let _ = state.key_down(CaptureKey::Modifier(ManualHotkeyChord::ALT));
        state.modifier_up(ManualHotkeyChord::ALT);
        assert_eq!(
            state.key_down(CaptureKey::Function(24)),
            CaptureOutcome::Invalid
        );
        assert_eq!(
            state.key_down(CaptureKey::Unsupported),
            CaptureOutcome::Invalid
        );
        assert_eq!(
            state.key_down(CaptureKey::Backspace),
            CaptureOutcome::Clear(ManualHotkeyAction::RecordingStop)
        );
    }

    #[test]
    fn explicit_lifecycle_cancel_clears_hidden_recorder_state() {
        let mut state = HotkeyCaptureState::default();
        state.begin(ManualHotkeyAction::ScreenshotRegion);
        let _ = state.key_down(CaptureKey::Modifier(ManualHotkeyChord::CTRL));
        assert_eq!(state.cancel(), Some(ManualHotkeyAction::ScreenshotRegion));
        assert_eq!(state.active_action(), None);
        assert_eq!(
            state.key_down(CaptureKey::Character('R')),
            CaptureOutcome::Ignored
        );
    }
}
