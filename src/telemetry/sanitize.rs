//! Telemetry sanitizer owner: no typed text, clipboard values, or URLs.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum KeyCode {
    Character(char),
    Enter,
    Escape,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Tab,
    Backspace,
    Delete,
    Function(u8),
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct KeyModifiers(u8);

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum SanitizedKeySignal {
    KeyLabel(String),
    Shortcut(String),
    Typing { count: u16 },
    Ignored,
}

#[must_use]
pub(crate) fn sanitize_key(key: KeyCode, modifiers: KeyModifiers) -> SanitizedKeySignal {
    if modifiers.has_command_modifier() {
        return shortcut_signal(key, modifiers);
    }
    match key {
        KeyCode::Character(_) => SanitizedKeySignal::Typing { count: 1 },
        KeyCode::Unknown => SanitizedKeySignal::Ignored,
        safe_key => SanitizedKeySignal::KeyLabel(safe_label(safe_key)),
    }
}

fn shortcut_signal(key: KeyCode, modifiers: KeyModifiers) -> SanitizedKeySignal {
    let Some(key_label) = shortcut_key_label(key) else {
        return SanitizedKeySignal::Ignored;
    };
    let mut parts = Vec::new();
    if modifiers.contains(KeyModifiers::CTRL) {
        parts.push("Ctrl");
    }
    if modifiers.contains(KeyModifiers::ALT) {
        parts.push("Alt");
    }
    if modifiers.contains(KeyModifiers::SHIFT) {
        parts.push("Shift");
    }
    if modifiers.contains(KeyModifiers::META) {
        parts.push("Meta");
    }
    parts.push(&key_label);
    SanitizedKeySignal::Shortcut(parts.join("+"))
}

fn shortcut_key_label(key: KeyCode) -> Option<String> {
    match key {
        KeyCode::Character(character) if character.is_ascii_alphabetic() => {
            Some(character.to_ascii_uppercase().to_string())
        }
        KeyCode::Character(character) if character.is_ascii_digit() => Some(character.to_string()),
        KeyCode::Enter
        | KeyCode::Escape
        | KeyCode::ArrowUp
        | KeyCode::ArrowDown
        | KeyCode::ArrowLeft
        | KeyCode::ArrowRight
        | KeyCode::Tab
        | KeyCode::Backspace
        | KeyCode::Delete
        | KeyCode::Function(_) => Some(safe_label(key)),
        KeyCode::Character(_) | KeyCode::Unknown => None,
    }
}

fn safe_label(key: KeyCode) -> String {
    match key {
        KeyCode::Enter => "Enter".to_owned(),
        KeyCode::Escape => "Escape".to_owned(),
        KeyCode::ArrowUp => "ArrowUp".to_owned(),
        KeyCode::ArrowDown => "ArrowDown".to_owned(),
        KeyCode::ArrowLeft => "ArrowLeft".to_owned(),
        KeyCode::ArrowRight => "ArrowRight".to_owned(),
        KeyCode::Tab => "Tab".to_owned(),
        KeyCode::Backspace => "Backspace".to_owned(),
        KeyCode::Delete => "Delete".to_owned(),
        KeyCode::Function(index) if (1..=24).contains(&index) => format!("F{index}"),
        KeyCode::Function(_) | KeyCode::Unknown | KeyCode::Character(_) => "Unknown".to_owned(),
    }
}

impl KeyModifiers {
    pub(crate) const CTRL: Self = Self(0b0001);
    pub(crate) const SHIFT: Self = Self(0b0010);
    pub(crate) const ALT: Self = Self(0b0100);
    pub(crate) const META: Self = Self(0b1000);

    #[must_use]
    pub(crate) const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    const fn has_command_modifier(self) -> bool {
        self.contains(Self::CTRL) || self.contains(Self::ALT) || self.contains(Self::META)
    }

    const fn contains(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}
