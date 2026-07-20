//! Win32 low-level mouse hook adapter for recording telemetry.
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
    UI::WindowsAndMessaging::{
        CallNextHookEx, HHOOK, LLMHF_INJECTED, MSLLHOOKSTRUCT, SetWindowsHookExW,
        UnhookWindowsHookEx, WH_MOUSE_LL, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_MOUSEMOVE,
        WM_RBUTTONDOWN,
    },
};

use super::super::hook_queue::{TelemetryHookEventQueue, TelemetryHookQueueSnapshot};
use super::mouse::{WindowsMouseButton, WindowsMouseEvent, WindowsMouseEventKind};

const MOUSE_HOOK_QUEUE_CAPACITY: usize = 1024;

static MOUSE_HOOK_STATE: OnceLock<Mutex<MouseHookState>> = OnceLock::new();
static MOUSE_HOOK_LOCK_DROPS: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
struct MouseHookState {
    hook_installed: bool,
    queue: TelemetryHookEventQueue<WindowsMouseEvent>,
}

impl Default for MouseHookState {
    fn default() -> Self {
        Self {
            hook_installed: false,
            queue: TelemetryHookEventQueue::new(MOUSE_HOOK_QUEUE_CAPACITY),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowsMouseTelemetryHookError {
    AlreadyInstalled,
    InstallFailed,
}

#[derive(Debug)]
pub(crate) struct WindowsMouseTelemetryHook {
    handle: HHOOK,
}

impl WindowsMouseTelemetryHook {
    pub(crate) fn install() -> Result<Self, WindowsMouseTelemetryHookError> {
        let state = mouse_hook_state();
        {
            let mut state = state
                .lock()
                .map_err(|_| WindowsMouseTelemetryHookError::InstallFailed)?;
            if state.hook_installed {
                return Err(WindowsMouseTelemetryHookError::AlreadyInstalled);
            }
            state.queue.clear();
            state.hook_installed = true;
        }

        // SAFETY: installs a low-level mouse hook for this process callback. The callback copies
        // scalar pointer data immediately, does not block, and stores bounded sanitized events.
        let handle = match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), None, 0) }
        {
            Ok(handle) => handle,
            Err(_) => {
                clear_installed_flag();
                return Err(WindowsMouseTelemetryHookError::InstallFailed);
            }
        };
        Ok(Self { handle })
    }

    pub(crate) fn drain_events(
        &mut self,
    ) -> io::Result<TelemetryHookQueueSnapshot<WindowsMouseEvent>> {
        mouse_hook_state()
            .lock()
            .map(|mut state| {
                let mut snapshot = state.queue.drain();
                snapshot.dropped_events = snapshot
                    .dropped_events
                    .saturating_add(MOUSE_HOOK_LOCK_DROPS.swap(0, Ordering::Relaxed));
                snapshot
            })
            .map_err(|_| io::Error::other("mouse hook state poisoned"))
    }
}

impl Drop for WindowsMouseTelemetryHook {
    fn drop(&mut self) {
        // SAFETY: RAII owner unhooks the HHOOK returned by SetWindowsHookExW exactly once.
        let _ = unsafe { UnhookWindowsHookEx(self.handle) };
        clear_installed_flag();
    }
}

unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        // SAFETY: for WH_MOUSE_LL callbacks, Windows documents LPARAM as a valid pointer to
        // MSLLHOOKSTRUCT for the duration of the callback. We copy only scalar fields immediately.
        let event = unsafe { mouse_event_from_hook(wparam, lparam) };
        if let Some(event) = event {
            push_mouse_event(event);
        }
    }
    // SAFETY: pass-through to the next hook preserves normal system hook dispatch; no HHOOK value
    // is required for low-level hook chaining.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe fn mouse_event_from_hook(wparam: WPARAM, lparam: LPARAM) -> Option<WindowsMouseEvent> {
    if lparam.0 == 0 {
        return None;
    }
    // SAFETY: caller guarantees LPARAM points to MSLLHOOKSTRUCT for this callback invocation.
    let hook = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
    if hook.flags & LLMHF_INJECTED != 0 {
        return None;
    }
    let kind = match u32::try_from(wparam.0) {
        Ok(WM_MOUSEMOVE) => WindowsMouseEventKind::Move,
        Ok(WM_LBUTTONDOWN) => WindowsMouseEventKind::Click(WindowsMouseButton::Left),
        Ok(WM_RBUTTONDOWN) => WindowsMouseEventKind::Click(WindowsMouseButton::Right),
        Ok(WM_MBUTTONDOWN) => WindowsMouseEventKind::Click(WindowsMouseButton::Middle),
        _ => return None,
    };
    Some(WindowsMouseEvent {
        x: hook.pt.x,
        y: hook.pt.y,
        kind,
    })
}

fn push_mouse_event(event: WindowsMouseEvent) {
    match mouse_hook_state().try_lock() {
        Ok(mut state) => state.queue.push(event),
        Err(_) => {
            MOUSE_HOOK_LOCK_DROPS.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn mouse_hook_state() -> &'static Mutex<MouseHookState> {
    MOUSE_HOOK_STATE.get_or_init(|| Mutex::new(MouseHookState::default()))
}

fn clear_installed_flag() {
    if let Ok(mut state) = mouse_hook_state().lock() {
        state.hook_installed = false;
        state.queue.clear();
    }
    MOUSE_HOOK_LOCK_DROPS.store(0, Ordering::Relaxed);
}
