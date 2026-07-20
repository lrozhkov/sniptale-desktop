//! Hotkey-to-action dispatch owner.

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum HotkeyKey {
    Character(char),
    Function(u8),
    PrintScreen,
}

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct HotkeyModifiers(u8);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct HotkeyChord {
    key: HotkeyKey,
    modifiers: HotkeyModifiers,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HotkeyBinding {
    pub(crate) hotkey: HotkeyChord,
    pub(crate) action_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HotkeyError {
    EmptyActionId,
    MissingModifier,
    InvalidFunctionKey,
    InvalidShortcutLabel,
    DuplicateHotkey,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct HotkeyDispatchMap {
    bindings: Vec<HotkeyBinding>,
}

impl HotkeyDispatchMap {
    pub(crate) fn new(mut bindings: Vec<HotkeyBinding>) -> Result<Self, HotkeyError> {
        for binding in &bindings {
            validate_binding(binding)?;
        }
        for binding in &mut bindings {
            binding.hotkey = binding.hotkey.normalized();
        }
        for left_index in 0..bindings.len() {
            for right in &bindings[left_index + 1..] {
                if bindings[left_index].hotkey == right.hotkey {
                    return Err(HotkeyError::DuplicateHotkey);
                }
            }
        }
        Ok(Self { bindings })
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) fn resolve(&self, hotkey: HotkeyChord) -> Option<&str> {
        let normalized = hotkey.normalized();
        self.bindings
            .iter()
            .find(|binding| binding.hotkey == normalized)
            .map(|binding| binding.action_id.as_str())
    }
}

fn validate_binding(binding: &HotkeyBinding) -> Result<(), HotkeyError> {
    if binding.action_id.is_empty() {
        return Err(HotkeyError::EmptyActionId);
    }
    if binding.hotkey.modifiers.is_empty() && binding.hotkey.key != HotkeyKey::PrintScreen {
        return Err(HotkeyError::MissingModifier);
    }
    if let HotkeyKey::Function(index) = binding.hotkey.key
        && !(1..=24).contains(&index)
    {
        return Err(HotkeyError::InvalidFunctionKey);
    }
    Ok(())
}

impl HotkeyKey {
    #[must_use]
    pub(crate) const fn with_modifiers(self, modifiers: HotkeyModifiers) -> HotkeyChord {
        HotkeyChord {
            key: self,
            modifiers,
        }
    }

    fn normalized(self) -> Self {
        match self {
            Self::Character(character) => Self::Character(character.to_ascii_uppercase()),
            Self::Function(index) => Self::Function(index),
            Self::PrintScreen => Self::PrintScreen,
        }
    }
}

impl HotkeyChord {
    pub(crate) fn normalized(self) -> Self {
        Self {
            key: self.key.normalized(),
            modifiers: self.modifiers,
        }
    }

    #[cfg(any(windows, test))]
    pub(crate) const fn key(self) -> HotkeyKey {
        self.key
    }

    #[cfg(windows)]
    pub(crate) const fn modifiers(self) -> HotkeyModifiers {
        self.modifiers
    }
}

impl HotkeyModifiers {
    pub(crate) const CTRL: Self = Self(0b0001);
    pub(crate) const SHIFT: Self = Self(0b0010);
    pub(crate) const ALT: Self = Self(0b0100);
    pub(crate) const META: Self = Self(0b1000);

    #[must_use]
    pub(crate) const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    const fn is_empty(self) -> bool {
        self.0 == 0
    }

    #[cfg(windows)]
    pub(crate) const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}
