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
    System::SystemInformation::GetTickCount64,
    UI::WindowsAndMessaging::{
        CURSOR_SHOWING, CURSORINFO, CallNextHookEx, GetCursorInfo, HHOOK, LLMHF_INJECTED, MSG,
        MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW, SetWindowsHookExW, UnhookWindowsHookEx,
        WH_MOUSE_LL, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE,
        WM_RBUTTONDOWN, WM_RBUTTONUP, WM_XBUTTONDOWN, WM_XBUTTONUP, XBUTTON1, XBUTTON2,
    },
};

use super::super::hook_queue::TelemetryHookEventQueue;
use super::mouse::{
    WindowsMouseButton, WindowsMouseEvent, WindowsMouseEventKind, WindowsMouseSnapshot,
};

const MOUSE_HOOK_QUEUE_CAPACITY: usize = 1024;

pub(crate) fn current_boot_ms() -> u64 {
    // SAFETY: GetTickCount64 has no pointer or resource arguments and returns a scalar boot clock.
    unsafe { GetTickCount64() }
}

pub(crate) fn pump_current_thread_hook_messages() {
    let mut message = MSG::default();
    // SAFETY: called on the hook-installing telemetry thread before taking queue locks.
    // PeekMessage dispatches pending nonqueued sent messages, including low-level hook callbacks;
    // stack output remains live for the call and PM_NOREMOVE preserves unrelated posted messages.
    let _ = unsafe { PeekMessageW(&raw mut message, None, 0, 0, PM_NOREMOVE) };
}

static MOUSE_HOOK_STATE: OnceLock<Mutex<MouseHookState>> = OnceLock::new();
static MOUSE_HOOK_LOCK_DROPS: AtomicU64 = AtomicU64::new(0);
static MOUSE_HOOK_NEXT_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
            MOUSE_HOOK_NEXT_SEQUENCE.store(0, Ordering::Relaxed);
            state.hook_installed = true;
        }

        // SAFETY: installs a low-level mouse hook for this process callback. The callback copies
        // scalar pointer data immediately, does not block, and stores bounded sanitized events.
        let Ok(handle) =
            (unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook_proc), None, 0) })
        else {
            clear_installed_flag();
            return Err(WindowsMouseTelemetryHookError::InstallFailed);
        };
        Ok(Self { handle })
    }

    pub(crate) fn drain_events(
        &mut self,
        source_cutoff_ms: Option<u64>,
    ) -> io::Result<WindowsMouseSnapshot> {
        if self.handle.0.is_null() {
            return Err(io::Error::other("telemetry hook is unavailable"));
        }
        mouse_hook_state()
            .lock()
            .map(|mut state| {
                let mut snapshot = state.queue.drain_while(|event| {
                    source_cutoff_ms.is_none_or(|cutoff| event.source_time_ms <= cutoff)
                });
                snapshot.dropped_events = snapshot
                    .dropped_events
                    .saturating_add(MOUSE_HOOK_LOCK_DROPS.swap(0, Ordering::Relaxed));
                WindowsMouseSnapshot {
                    events: snapshot.events,
                    dropped_events: snapshot.dropped_events,
                    last_observation_seq: state.queue.front().map_or_else(
                        || {
                            MOUSE_HOOK_NEXT_SEQUENCE
                                .load(Ordering::Relaxed)
                                .checked_sub(1)
                        },
                        |pending| pending.sequence.checked_sub(1),
                    ),
                }
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
        Ok(WM_LBUTTONDOWN) => WindowsMouseEventKind::Down(WindowsMouseButton::Left),
        Ok(WM_LBUTTONUP) => WindowsMouseEventKind::Up(WindowsMouseButton::Left),
        Ok(WM_RBUTTONDOWN) => WindowsMouseEventKind::Down(WindowsMouseButton::Right),
        Ok(WM_RBUTTONUP) => WindowsMouseEventKind::Up(WindowsMouseButton::Right),
        Ok(WM_MBUTTONDOWN) => WindowsMouseEventKind::Down(WindowsMouseButton::Middle),
        Ok(WM_MBUTTONUP) => WindowsMouseEventKind::Up(WindowsMouseButton::Middle),
        Ok(message @ (WM_XBUTTONDOWN | WM_XBUTTONUP)) => {
            let button = match u16::try_from(hook.mouseData >> 16).ok()? {
                XBUTTON1 => WindowsMouseButton::X1,
                XBUTTON2 => WindowsMouseButton::X2,
                _ => return None,
            };
            if message == WM_XBUTTONDOWN {
                WindowsMouseEventKind::Down(button)
            } else {
                WindowsMouseEventKind::Up(button)
            }
        }
        _ => return None,
    };
    Some(WindowsMouseEvent {
        sequence: MOUSE_HOOK_NEXT_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        source_time_ms: crate::telemetry::trajectory_clock::extend_windows_message_time(
            hook.time,
            current_boot_ms(),
        )
        .ok()?,
        visible: cursor_visibility(),
        x: hook.pt.x,
        y: hook.pt.y,
        kind,
    })
}

fn cursor_visibility() -> Option<bool> {
    let mut info = CURSORINFO {
        cbSize: u32::try_from(size_of::<CURSORINFO>()).ok()?,
        ..CURSORINFO::default()
    };
    // SAFETY: cbSize matches this initialized stack output. Its borrowed HCURSOR is neither
    // exposed nor destroyed; only the visibility flag is copied before the callback returns.
    unsafe { GetCursorInfo(&raw mut info) }.ok()?;
    Some(info.flags.0 & CURSOR_SHOWING.0 != 0)
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
    MOUSE_HOOK_NEXT_SEQUENCE.store(0, Ordering::Relaxed);
}
