//! Win32 low-level keyboard hook adapter for recording telemetry.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::{
    io,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use windows::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    UI::WindowsAndMessaging::{
        CallNextHookEx, HHOOK, KBDLLHOOKSTRUCT, LLKHF_ALTDOWN, LLKHF_INJECTED,
        LLKHF_LOWER_IL_INJECTED, LLKHF_UP, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL,
        WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
    },
};

use super::super::hook_queue::{TelemetryHookEventQueue, TelemetryHookQueueSnapshot};
use super::key::{
    WindowsKeyEvent, WindowsKeyModifiers, WindowsKeyObservation, WindowsKeyTransition,
};

const KEYBOARD_HOOK_QUEUE_CAPACITY: usize = 512;

static KEYBOARD_HOOK_STATE: OnceLock<Mutex<KeyboardHookState>> = OnceLock::new();
static KEYBOARD_HOOK_LOCK_DROPS: AtomicU64 = AtomicU64::new(0);
static KEYBOARD_HOOK_NEXT_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static KEYBOARD_HOOK_STATE_UNCERTAIN: AtomicBool = AtomicBool::new(false);

#[derive(Debug)]
struct KeyboardHookState {
    hook_installed: bool,
    queue: TelemetryHookEventQueue<WindowsKeyObservation>,
    keys: [WindowsKeyTransition; 256],
}

impl Default for KeyboardHookState {
    fn default() -> Self {
        Self {
            hook_installed: false,
            queue: TelemetryHookEventQueue::new(KEYBOARD_HOOK_QUEUE_CAPACITY),
            keys: [WindowsKeyTransition::Unknown; 256],
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
            state.keys.fill(WindowsKeyTransition::Unknown);
            KEYBOARD_HOOK_NEXT_SEQUENCE.store(0, Ordering::Relaxed);
            KEYBOARD_HOOK_STATE_UNCERTAIN.store(false, Ordering::Relaxed);
            state.hook_installed = true;
        }

        // SAFETY: installs a low-level keyboard hook for this process callback. The callback does
        // not retain raw LPARAM pointers, does not block, and only writes typed events into a
        // bounded queue owned by this adapter.
        let Ok(handle) =
            (unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0) })
        else {
            clear_installed_flag();
            return Err(WindowsKeyboardTelemetryHookError::InstallFailed);
        };
        Ok(Self { handle })
    }

    pub(crate) fn drain_events(
        &mut self,
        source_cutoff_ms: Option<u64>,
    ) -> io::Result<TelemetryHookQueueSnapshot<WindowsKeyObservation>> {
        if self.handle.0.is_null() {
            return Err(io::Error::other("telemetry hook is unavailable"));
        }
        keyboard_hook_state()
            .lock()
            .map(|mut state| {
                let mut snapshot = state.queue.drain_while(|event| {
                    source_cutoff_ms.is_none_or(|cutoff| event.source_time_ms <= cutoff)
                });
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
    if code >= 0 && is_key_message(wparam) {
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

unsafe fn keyboard_event_from_hook(lparam: LPARAM) -> Option<RawKeyboardFact> {
    if lparam.0 == 0 {
        return None;
    }
    // SAFETY: caller guarantees LPARAM points to KBDLLHOOKSTRUCT for this callback invocation.
    let hook = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
    if hook.flags.contains(LLKHF_INJECTED) || hook.flags.contains(LLKHF_LOWER_IL_INJECTED) {
        KEYBOARD_HOOK_STATE_UNCERTAIN.store(true, Ordering::Relaxed);
        return None;
    }
    let source_time_ms = crate::telemetry::trajectory_clock::extend_windows_message_time(
        hook.time,
        super::mouse_hook::current_boot_ms(),
    )
    .ok()?;
    Some(RawKeyboardFact {
        virtual_key: hook.vkCode,
        alt_down: hook.flags.contains(LLKHF_ALTDOWN),
        source_time_ms,
        up: hook.flags.contains(LLKHF_UP),
    })
}

fn is_key_message(wparam: WPARAM) -> bool {
    matches!(
        u32::try_from(wparam.0),
        Ok(WM_KEYDOWN | WM_SYSKEYDOWN | WM_KEYUP | WM_SYSKEYUP)
    )
}

#[derive(Clone, Copy)]
struct RawKeyboardFact {
    virtual_key: u32,
    alt_down: bool,
    source_time_ms: u64,
    up: bool,
}

fn push_keyboard_event(fact: RawKeyboardFact) {
    let Ok(key) = usize::try_from(fact.virtual_key) else {
        return;
    };
    if key >= 256 {
        return;
    }
    let sequence = KEYBOARD_HOOK_NEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut state) = keyboard_hook_state().try_lock() {
        if KEYBOARD_HOOK_STATE_UNCERTAIN.swap(false, Ordering::Relaxed) {
            state.keys.fill(WindowsKeyTransition::Unknown);
        }
        let phase = state.keys[key].observe(fact.up);
        let modifiers = WindowsKeyModifiers::from_observed_keys(&state.keys, fact.alt_down);
        state.queue.push(WindowsKeyObservation {
            event: WindowsKeyEvent {
                virtual_key: fact.virtual_key,
                modifiers,
            },
            source_time_ms: fact.source_time_ms,
            sequence,
            phase,
        });
    } else {
        KEYBOARD_HOOK_STATE_UNCERTAIN.store(true, Ordering::Relaxed);
        KEYBOARD_HOOK_LOCK_DROPS.fetch_add(1, Ordering::Relaxed);
    }
}

fn keyboard_hook_state() -> &'static Mutex<KeyboardHookState> {
    KEYBOARD_HOOK_STATE.get_or_init(|| Mutex::new(KeyboardHookState::default()))
}

fn clear_installed_flag() {
    if let Ok(mut state) = keyboard_hook_state().lock() {
        state.hook_installed = false;
        state.queue.clear();
        state.keys.fill(WindowsKeyTransition::Unknown);
    }
    KEYBOARD_HOOK_LOCK_DROPS.store(0, Ordering::Relaxed);
}
