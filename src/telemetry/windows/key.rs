//! Windows virtual-key mapping for recording telemetry.

use super::super::sanitize::{KeyCode, KeyModifiers, SanitizedKeySignal, sanitize_key};

const VK_BACK: u32 = 0x08;
const VK_TAB: u32 = 0x09;
const VK_RETURN: u32 = 0x0D;
const VK_ESCAPE: u32 = 0x1B;
const VK_SPACE: u32 = 0x20;
const VK_LEFT: u32 = 0x25;
const VK_UP: u32 = 0x26;
const VK_RIGHT: u32 = 0x27;
const VK_DOWN: u32 = 0x28;
const VK_DELETE: u32 = 0x2E;
const VK_0: u32 = 0x30;
const VK_9: u32 = 0x39;
const VK_A: u32 = 0x41;
const VK_Z: u32 = 0x5A;
const VK_F1: u32 = 0x70;
const VK_F24: u32 = 0x87;
const VK_OEM_1: u32 = 0xBA;
const VK_OEM_8: u32 = 0xDF;
const VK_OEM_102: u32 = 0xE2;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct WindowsKeyModifiers(u8);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WindowsKeyEvent {
    pub(crate) virtual_key: u32,
    pub(crate) modifiers: WindowsKeyModifiers,
}

pub(crate) fn sanitize_windows_key_event(event: WindowsKeyEvent) -> SanitizedKeySignal {
    sanitize_key(
        key_code_for_virtual_key(event.virtual_key, event.modifiers.has_command_modifier()),
        event.modifiers.into_key_modifiers(),
    )
}

fn key_code_for_virtual_key(virtual_key: u32, command_modified: bool) -> KeyCode {
    match virtual_key {
        VK_RETURN => KeyCode::Enter,
        VK_ESCAPE => KeyCode::Escape,
        VK_TAB => KeyCode::Tab,
        VK_BACK => KeyCode::Backspace,
        VK_DELETE => KeyCode::Delete,
        VK_UP => KeyCode::ArrowUp,
        VK_DOWN => KeyCode::ArrowDown,
        VK_LEFT => KeyCode::ArrowLeft,
        VK_RIGHT => KeyCode::ArrowRight,
        VK_F1..=VK_F24 => KeyCode::Function(u8::try_from(virtual_key - VK_F1 + 1).unwrap_or(0)),
        VK_A..=VK_Z if command_modified => {
            let ascii = u8::try_from(virtual_key).unwrap_or(b'A');
            KeyCode::Character(char::from(ascii).to_ascii_lowercase())
        }
        VK_0..=VK_9 if command_modified => {
            let ascii = u8::try_from(virtual_key).unwrap_or(b'0');
            KeyCode::Character(char::from(ascii))
        }
        VK_A..=VK_Z | VK_0..=VK_9 | VK_SPACE | VK_OEM_1..=VK_OEM_8 | VK_OEM_102 => {
            KeyCode::Character(' ')
        }
        _ => KeyCode::Unknown,
    }
}

impl WindowsKeyModifiers {
    pub(crate) const CTRL: Self = Self(0b0001);
    pub(crate) const SHIFT: Self = Self(0b0010);
    pub(crate) const ALT: Self = Self(0b0100);
    pub(crate) const META: Self = Self(0b1000);

    pub(crate) const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    const fn has_command_modifier(self) -> bool {
        self.contains(Self::CTRL) || self.contains(Self::ALT) || self.contains(Self::META)
    }

    fn into_key_modifiers(self) -> KeyModifiers {
        let mut modifiers = KeyModifiers::default();
        if self.contains(Self::CTRL) {
            modifiers = modifiers.with(KeyModifiers::CTRL);
        }
        if self.contains(Self::SHIFT) {
            modifiers = modifiers.with(KeyModifiers::SHIFT);
        }
        if self.contains(Self::ALT) {
            modifiers = modifiers.with(KeyModifiers::ALT);
        }
        if self.contains(Self::META) {
            modifiers = modifiers.with(KeyModifiers::META);
        }
        modifiers
    }

    const fn contains(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}
