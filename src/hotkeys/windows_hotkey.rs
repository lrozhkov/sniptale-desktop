//! Win32 global hotkey adapter.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::io;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey,
    VK_F1, VK_SNAPSHOT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    MSG, PM_NOREMOVE, PM_REMOVE, PeekMessageW, WM_HOTKEY, WM_USER,
};

use super::{
    dispatch::{HotkeyChord, HotkeyKey, HotkeyModifiers},
    registry::RegisteredHotkey,
    runtime::{HotkeyEventSource, HotkeyRegistrar, HotkeyRuntimeError},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct WindowsHotkeyRegistrar;

impl HotkeyRegistrar for WindowsHotkeyRegistrar {
    type Handle = WindowsHotkeyRegistration;

    fn register(&mut self, hotkey: &RegisteredHotkey) -> Result<Self::Handle, HotkeyRuntimeError> {
        WindowsHotkeyRegistration::register(hotkey).map_err(|error| {
            HotkeyRuntimeError::RegisterFailedWithOsError {
                registration_id: hotkey.registration_id,
                os_error_code: win32_error_code(&error),
            }
        })
    }
}

impl HotkeyEventSource for WindowsHotkeyRegistrar {
    fn drain_hotkey_events(&mut self) -> Vec<u32> {
        let mut events = Vec::new();
        loop {
            let mut message = MSG::default();
            // SAFETY: this drains only the current thread message queue; the MSG storage lives for
            // the call and no window handle is retained.
            let has_message =
                unsafe { PeekMessageW(&mut message, None, WM_HOTKEY, WM_HOTKEY, PM_REMOVE) }
                    .as_bool();
            if !has_message {
                break;
            }
            if let Ok(registration_id) = u32::try_from(message.wParam.0) {
                events.push(registration_id);
            }
        }
        events
    }
}

#[derive(Debug)]
pub(crate) struct WindowsHotkeyRegistration {
    id: i32,
}

impl WindowsHotkeyRegistration {
    pub(crate) fn register(hotkey: &RegisteredHotkey) -> io::Result<Self> {
        let id = i32::try_from(hotkey.registration_id)
            .map_err(|_| io::Error::other("hotkey registration id exceeds i32"))?;
        let modifiers = win32_modifiers(hotkey.hotkey);
        let virtual_key = win32_virtual_key(hotkey.hotkey)?;
        let mut message = MSG::default();
        // SAFETY: a NULL-window RegisterHotKey posts WM_HOTKEY to this worker thread. A
        // non-removing peek creates that queue before registration without dispatching messages.
        unsafe {
            let _ = PeekMessageW(&mut message, None, WM_USER, WM_USER, PM_NOREMOVE);
        }
        // SAFETY: hwnd=None registers the hotkey for the current thread message queue; id is a
        // bounded positive registry id; modifiers/key are derived from validated HotkeyChord data.
        unsafe { RegisterHotKey(None, id, modifiers, virtual_key) }
            .map_err(|error| to_io_error(&error))?;
        Ok(Self { id })
    }
}

impl Drop for WindowsHotkeyRegistration {
    fn drop(&mut self) {
        // SAFETY: this RAII owner unregisters the same thread/global hotkey id it registered.
        let _ = unsafe { UnregisterHotKey(None, self.id) };
    }
}

fn win32_modifiers(chord: HotkeyChord) -> HOT_KEY_MODIFIERS {
    let modifiers = chord.modifiers();
    let mut value = 0;
    if modifiers.contains(HotkeyModifiers::ALT) {
        value |= MOD_ALT.0;
    }
    if modifiers.contains(HotkeyModifiers::CTRL) {
        value |= MOD_CONTROL.0;
    }
    if modifiers.contains(HotkeyModifiers::SHIFT) {
        value |= MOD_SHIFT.0;
    }
    if modifiers.contains(HotkeyModifiers::META) {
        value |= MOD_WIN.0;
    }
    HOT_KEY_MODIFIERS(value)
}

fn win32_virtual_key(chord: HotkeyChord) -> io::Result<u32> {
    match chord.key() {
        HotkeyKey::Character(character) if character.is_ascii_alphanumeric() => {
            Ok(u32::from(character.to_ascii_uppercase()))
        }
        HotkeyKey::Function(index) if (1..=24).contains(&index) => {
            Ok(u32::from(VK_F1.0) + u32::from(index - 1))
        }
        HotkeyKey::PrintScreen => Ok(u32::from(VK_SNAPSHOT.0)),
        HotkeyKey::Character(_) | HotkeyKey::Function(_) => {
            Err(io::Error::other("unsupported hotkey key"))
        }
    }
}

fn to_io_error(error: &windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error(error.code().0)
}

fn win32_error_code(error: &io::Error) -> u32 {
    error
        .raw_os_error()
        .map(|code| u32::from_ne_bytes(code.to_ne_bytes()) & 0xffff)
        .unwrap_or_default()
}
