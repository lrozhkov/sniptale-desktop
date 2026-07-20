//! Hotkey bindings derived from accepted extension settings.

use super::dispatch::{HotkeyBinding, HotkeyChord, HotkeyError, HotkeyKey, HotkeyModifiers};
use crate::settings::ActionCatalog;
#[cfg(any(windows, test))]
use std::collections::BTreeMap;

pub(crate) fn bindings_from_action_catalog(
    catalog: &ActionCatalog,
) -> Result<Vec<HotkeyBinding>, HotkeyError> {
    let mut bindings = Vec::new();
    for action in catalog.entries() {
        let Some(label) = action.shortcut_label.as_deref() else {
            continue;
        };
        let label = label.trim();
        if label.is_empty() || !action.enabled {
            continue;
        }
        bindings.push(HotkeyBinding {
            hotkey: parse_shortcut_label(label)?,
            action_id: action.id.clone(),
        });
    }
    super::dispatch::HotkeyDispatchMap::new(bindings.clone())?;
    Ok(bindings)
}

#[cfg(any(windows, test))]
pub(crate) fn bindings_from_manual_hotkeys(
    hotkeys: &BTreeMap<String, String>,
) -> Result<Vec<HotkeyBinding>, HotkeyError> {
    let mut bindings = Vec::new();
    for (action_id, label) in hotkeys {
        if crate::settings::ManualHotkeyAction::from_id(action_id).is_none() {
            return Err(HotkeyError::InvalidShortcutLabel);
        }
        bindings.push(HotkeyBinding {
            hotkey: manual_chord(label)?,
            action_id: action_id.clone(),
        });
    }
    super::dispatch::HotkeyDispatchMap::new(bindings.clone())?;
    Ok(bindings)
}

#[cfg(any(windows, test))]
fn manual_chord(label: &str) -> Result<HotkeyChord, HotkeyError> {
    let parsed = crate::settings::parse_manual_hotkey_chord(label)
        .ok_or(HotkeyError::InvalidShortcutLabel)?;
    let mut modifiers = HotkeyModifiers::default();
    for (persisted, runtime) in [
        (
            crate::settings::ManualHotkeyChord::CTRL,
            HotkeyModifiers::CTRL,
        ),
        (
            crate::settings::ManualHotkeyChord::SHIFT,
            HotkeyModifiers::SHIFT,
        ),
        (
            crate::settings::ManualHotkeyChord::ALT,
            HotkeyModifiers::ALT,
        ),
        (
            crate::settings::ManualHotkeyChord::META,
            HotkeyModifiers::META,
        ),
    ] {
        if parsed.contains(persisted) {
            modifiers = modifiers.with(runtime);
        }
    }
    let key = match parsed.key {
        crate::settings::ManualHotkeyKey::Character(character) => HotkeyKey::Character(character),
        crate::settings::ManualHotkeyKey::Function(index) => HotkeyKey::Function(index),
        crate::settings::ManualHotkeyKey::PrintScreen => HotkeyKey::PrintScreen,
    };
    Ok(key.with_modifiers(modifiers))
}

pub(crate) fn parse_shortcut_label(label: &str) -> Result<HotkeyChord, HotkeyError> {
    let mut modifiers = HotkeyModifiers::default();
    let mut key = None;
    for part in label.split('+') {
        let token = part.trim();
        if token.is_empty() {
            return Err(HotkeyError::InvalidShortcutLabel);
        }
        match token.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => modifiers = modifiers.with(HotkeyModifiers::CTRL),
            "shift" => modifiers = modifiers.with(HotkeyModifiers::SHIFT),
            "alt" => modifiers = modifiers.with(HotkeyModifiers::ALT),
            "meta" | "win" | "windows" | "cmd" => modifiers = modifiers.with(HotkeyModifiers::META),
            "printscreen" | "print-screen" | "print screen" | "prtsc" => {
                set_key(&mut key, HotkeyKey::PrintScreen)?;
            }
            value => set_key(&mut key, parse_key(value)?)?,
        }
    }
    let Some(key) = key else {
        return Err(HotkeyError::InvalidShortcutLabel);
    };
    Ok(key.with_modifiers(modifiers).normalized())
}

fn parse_key(token: &str) -> Result<HotkeyKey, HotkeyError> {
    if let Some(index) = token
        .strip_prefix('f')
        .and_then(|value| value.parse::<u8>().ok())
    {
        return Ok(HotkeyKey::Function(index));
    }
    let mut chars = token.chars();
    let Some(character) = chars.next() else {
        return Err(HotkeyError::InvalidShortcutLabel);
    };
    if chars.next().is_none() && character.is_ascii_alphanumeric() {
        Ok(HotkeyKey::Character(character))
    } else {
        Err(HotkeyError::InvalidShortcutLabel)
    }
}

fn set_key(slot: &mut Option<HotkeyKey>, key: HotkeyKey) -> Result<(), HotkeyError> {
    if slot.replace(key).is_some() {
        Err(HotkeyError::InvalidShortcutLabel)
    } else {
        Ok(())
    }
}
