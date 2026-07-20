//! Platform-neutral grammar for persisted Manual shortcut chords.

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum ManualHotkeyKey {
    Character(char),
    Function(u8),
    PrintScreen,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct ManualHotkeyChord {
    pub(crate) key: ManualHotkeyKey,
    modifiers: u8,
}

impl ManualHotkeyChord {
    pub(crate) const CTRL: u8 = 0b0001;
    pub(crate) const SHIFT: u8 = 0b0010;
    pub(crate) const ALT: u8 = 0b0100;
    pub(crate) const META: u8 = 0b1000;
    const ALL_MODIFIERS: u8 = Self::CTRL | Self::SHIFT | Self::ALT | Self::META;

    #[must_use]
    pub(crate) const fn new(key: ManualHotkeyKey, modifiers: u8) -> Option<Self> {
        if modifiers & !Self::ALL_MODIFIERS != 0
            || (!matches!(key, ManualHotkeyKey::PrintScreen) && modifiers == 0)
        {
            None
        } else {
            Some(Self { key, modifiers })
        }
    }

    #[cfg(any(windows, test))]
    pub(crate) const fn contains(self, modifier: u8) -> bool {
        self.modifiers & modifier == modifier
    }

    #[must_use]
    #[cfg(any(windows, test))]
    pub(crate) fn canonical_label(self) -> String {
        let mut parts = Vec::with_capacity(5);
        for (modifier, label) in [
            (Self::CTRL, "Ctrl"),
            (Self::SHIFT, "Shift"),
            (Self::ALT, "Alt"),
            (Self::META, "Win"),
        ] {
            if self.contains(modifier) {
                parts.push(label.to_owned());
            }
        }
        parts.push(match self.key {
            ManualHotkeyKey::Character(character) => character.to_string(),
            ManualHotkeyKey::Function(index) => format!("F{index}"),
            ManualHotkeyKey::PrintScreen => "PrintScreen".to_owned(),
        });
        parts.join("+")
    }
}

pub(crate) fn parse_manual_hotkey_chord(value: &str) -> Option<ManualHotkeyChord> {
    let mut modifiers = 0;
    let mut key = None;
    for part in value.split('+') {
        let token = part.trim();
        if token.is_empty() {
            return None;
        }
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers |= ManualHotkeyChord::CTRL,
            "shift" => modifiers |= ManualHotkeyChord::SHIFT,
            "alt" => modifiers |= ManualHotkeyChord::ALT,
            "meta" | "win" | "windows" | "cmd" => {
                modifiers |= ManualHotkeyChord::META;
            }
            "printscreen" | "print-screen" | "print screen" | "prtsc" => {
                set_key(&mut key, ManualHotkeyKey::PrintScreen)?;
            }
            token => set_key(&mut key, parse_key(token)?)?,
        }
    }
    ManualHotkeyChord::new(key?, modifiers)
}

fn parse_key(token: &str) -> Option<ManualHotkeyKey> {
    if let Some(index) = token
        .strip_prefix('f')
        .and_then(|value| value.parse::<u8>().ok())
        .filter(|index| (1..=24).contains(index))
    {
        return Some(ManualHotkeyKey::Function(index));
    }
    let mut characters = token.chars();
    let character = characters.next()?;
    (characters.next().is_none() && character.is_ascii_alphanumeric())
        .then(|| ManualHotkeyKey::Character(character.to_ascii_uppercase()))
}

fn set_key(slot: &mut Option<ManualHotkeyKey>, key: ManualHotkeyKey) -> Option<()> {
    if slot.replace(key).is_some() {
        None
    } else {
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grammar_accepts_supported_chords_and_rejects_unsafe_forms() {
        assert!(parse_manual_hotkey_chord("Ctrl+Shift+S").is_some());
        assert!(parse_manual_hotkey_chord("PrintScreen").is_some());
        assert!(parse_manual_hotkey_chord("Ctrl+F24").is_some());
        assert!(parse_manual_hotkey_chord("S").is_none());
        assert!(parse_manual_hotkey_chord("Ctrl++S").is_none());
        assert!(parse_manual_hotkey_chord("Ctrl+F25").is_none());
        assert!(parse_manual_hotkey_chord("Ctrl+S+R").is_none());
    }

    #[test]
    fn canonical_formatter_normalizes_aliases_and_modifier_order() {
        assert_eq!(
            parse_manual_hotkey_chord("windows+alt+control+f12")
                .map(ManualHotkeyChord::canonical_label)
                .as_deref(),
            Some("Ctrl+Alt+Win+F12")
        );
        assert_eq!(
            parse_manual_hotkey_chord("prtsc")
                .map(ManualHotkeyChord::canonical_label)
                .as_deref(),
            Some("PrintScreen")
        );
    }
}
