//! Win32 low-level keyboard hook adapter for recording telemetry.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::{
    io,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use windows::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    UI::{
        Input::KeyboardAndMouse::GetAsyncKeyState,
        WindowsAndMessaging::{
            CallNextHookEx, HHOOK, KBDLLHOOKSTRUCT, LLKHF_ALTDOWN, LLKHF_INJECTED,
            LLKHF_LOWER_IL_INJECTED, LLKHF_UP, SetWindowsHookExW, UnhookWindowsHookEx,
            WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
        },
    },
};

use super::super::hook_queue::{TelemetryHookEventQueue, TelemetryHookQueueSnapshot};
use super::key::{WindowsKeyEvent, WindowsKeyModifiers};

const KEYBOARD_HOOK_QUEUE_CAPACITY: usize = 512;
const VK_CONTROL: i32 = 0x11;
const VK_SHIFT: i32 = 0x10;
const VK_LWIN: i32 = 0x5B;
const VK_RWIN: i32 = 0x5C;
const KEY_DOWN_MASK: i16 = i16::MIN;

static KEYBOARD_HOOK_STATE: OnceLock<Mutex<KeyboardHookState>> = OnceLock::new();
static KEYBOARD_HOOK_LOCK_DROPS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct KeyboardHookState {
    hook_installed: bool,
    queue: TelemetryHookEventQueue<WindowsKeyEvent>,
}

impl Default for KeyboardHookState {
    fn default() -> Self {
        Self {
            hook_installed: false,
            queue: TelemetryHookEventQueue::new(KEYBOARD_HOOK_QUEUE_CAPACITY),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowsKeyboardTelemetryHookError {
    AlreadyInstalled,
    InstallFailed,
}

#[derive(Debug)]
pub(crate) struct WindowsKeyboardTelemetryHook {
    handle: HHOOK,
}

impl WindowsKeyboardTelemetryHook {
    pub(crate) fn install() -> Result<Self, WindowsKeyboardTelemetryHookError> {
        let state = keyboard_hook_state();
        {
            let mut state = state
                .lock()
                .map_err(|_| WindowsKeyboardTelemetryHookError::InstallFailed)?;
            if state.hook_installed {
                return Err(WindowsKeyboardTelemetryHookError::AlreadyInstalled);
            }
            state.queue.clear();
            state.hook_installed = true;
        }

        // SAFETY: installs a low-level keyboard hook for this process callback. The callback does
        // not retain raw LPARAM pointers, does not block, and only writes typed events into a
        // bounded queue owned by this adapter.
        let handle =
            match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0) } {
                Ok(handle) => handle,
                Err(_) => {
                    clear_installed_flag();
                    return Err(WindowsKeyboardTelemetryHookError::InstallFailed);
                }
            };
        Ok(Self { handle })
    }

    pub(crate) fn drain_events(
        &mut self,
    ) -> io::Result<TelemetryHookQueueSnapshot<WindowsKeyEvent>> {
        keyboard_hook_state()
            .lock()
            .map(|mut state| {
                let mut snapshot = state.queue.drain();
                snapshot.dropped_events = snapshot
                    .dropped_events
                    .saturating_add(KEYBOARD_HOOK_LOCK_DROPS.swap(0, Ordering::Relaxed));
                snapshot
            })
            .map_err(|_| io::Error::other("keyboard hook state poisoned"))
    }
}

impl Drop for WindowsKeyboardTelemetryHook {
    fn drop(&mut self) {
        // SAFETY: RAII owner unhooks the HHOOK returned by SetWindowsHookExW exactly once.
        let _ = unsafe { UnhookWindowsHookEx(self.handle) };
        clear_installed_flag();
    }
}

unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && is_key_down_message(wparam) {
        // SAFETY: for WH_KEYBOARD_LL callbacks, Windows documents LPARAM as a valid pointer to
        // KBDLLHOOKSTRUCT for the duration of the callback. We copy only scalar fields immediately.
        let event = unsafe { keyboard_event_from_hook(lparam) };
        if let Some(event) = event {
            push_keyboard_event(event);
        }
    }
    // SAFETY: pass-through to the next hook preserves normal system hook dispatch; no HHOOK value
    // is required for low-level hook chaining.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe fn keyboard_event_from_hook(lparam: LPARAM) -> Option<WindowsKeyEvent> {
    if lparam.0 == 0 {
        return None;
    }
    // SAFETY: caller guarantees LPARAM points to KBDLLHOOKSTRUCT for this callback invocation.
    let hook = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
    if hook.flags.contains(LLKHF_UP)
        || hook.flags.contains(LLKHF_INJECTED)
        || hook.flags.contains(LLKHF_LOWER_IL_INJECTED)
    {
        return None;
    }
    Some(WindowsKeyEvent {
        virtual_key: hook.vkCode,
        modifiers: current_modifiers(hook.flags.contains(LLKHF_ALTDOWN)),
    })
}

fn is_key_down_message(wparam: WPARAM) -> bool {
    matches!(u32::try_from(wparam.0), Ok(WM_KEYDOWN | WM_SYSKEYDOWN))
}

fn current_modifiers(alt_down: bool) -> WindowsKeyModifiers {
    let mut modifiers = WindowsKeyModifiers::default();
    if key_is_down(VK_CONTROL) {
        modifiers = modifiers.with(WindowsKeyModifiers::CTRL);
    }
    if key_is_down(VK_SHIFT) {
        modifiers = modifiers.with(WindowsKeyModifiers::SHIFT);
    }
    if alt_down {
        modifiers = modifiers.with(WindowsKeyModifiers::ALT);
    }
    if key_is_down(VK_LWIN) || key_is_down(VK_RWIN) {
        modifiers = modifiers.with(WindowsKeyModifiers::META);
    }
    modifiers
}

fn key_is_down(virtual_key: i32) -> bool {
    // SAFETY: GetAsyncKeyState reads key state for a virtual-key code constant and returns a value
    // by copy; no pointers or handles cross this boundary.
    unsafe { GetAsyncKeyState(virtual_key) & KEY_DOWN_MASK != 0 }
}

fn push_keyboard_event(event: WindowsKeyEvent) {
    match keyboard_hook_state().try_lock() {
        Ok(mut state) => state.queue.push(event),
        Err(_) => {
            KEYBOARD_HOOK_LOCK_DROPS.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn keyboard_hook_state() -> &'static Mutex<KeyboardHookState> {
    KEYBOARD_HOOK_STATE.get_or_init(|| Mutex::new(KeyboardHookState::default()))
}

fn clear_installed_flag() {
    if let Ok(mut state) = keyboard_hook_state().lock() {
        state.hook_installed = false;
        state.queue.clear();
    }
    KEYBOARD_HOOK_LOCK_DROPS.store(0, Ordering::Relaxed);
}
