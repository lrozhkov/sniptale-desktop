//! Dedicated low-level hook dispatch. Telemetry processing never blocks this message loop.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, HHOOK, HOOKPROC, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx,
    PM_REMOVE, PeekMessageW, QS_ALLINPUT, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx,
    WINDOWS_HOOK_ID,
};

struct InstalledHook(HHOOK);

impl Drop for InstalledHook {
    fn drop(&mut self) {
        // SAFETY: the installing thread owns this hook and drops it exactly once.
        let _ = unsafe { UnhookWindowsHookEx(self.0) };
    }
}

#[derive(Debug)]
pub(super) struct HookThread {
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl HookThread {
    pub(super) fn install(kind: WINDOWS_HOOK_ID, callback: HOOKPROC) -> io::Result<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = stop.clone();
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = std::thread::Builder::new()
            .name("recording-input-hook".to_owned())
            .spawn(move || {
                // SAFETY: the static callback copies scalar facts into a bounded queue. The
                // returned handle stays on this thread and is unhooked before thread exit.
                let hook = unsafe { SetWindowsHookExW(kind, callback, None, 0) };
                let Ok(hook) = hook else {
                    let _ = sender.send(false);
                    return;
                };
                let hook = InstalledHook(hook);
                if sender.send(true).is_ok() {
                    dispatch_until_stopped(&worker_stop);
                }
                drop(hook);
            })?;
        if receiver.recv() != Ok(true) {
            let _ = worker.join();
            return Err(io::Error::other("telemetry hook installation failed"));
        }
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }

    pub(super) fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    pub(super) fn is_finished(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }
}

impl Drop for HookThread {
    fn drop(&mut self) {
        self.stop();
    }
}

fn dispatch_until_stopped(stop: &AtomicBool) {
    while !stop.load(Ordering::Acquire) {
        // SAFETY: no borrowed handles are supplied. Incoming input wakes this wait at once;
        // the 50 ms timeout bounds shutdown only and does not delay hook dispatch.
        let _ = unsafe { MsgWaitForMultipleObjectsEx(None, 50, QS_ALLINPUT, MWMO_INPUTAVAILABLE) };
        let mut message = MSG::default();
        // SAFETY: the stack output remains live. This dedicated thread owns its queue and
        // has no product windows. PeekMessage also dispatches nonqueued hook callbacks.
        while unsafe { PeekMessageW(&raw mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            // SAFETY: dispatch only initialized messages obtained from this thread's queue.
            unsafe {
                let _ = TranslateMessage(&raw const message);
                DispatchMessageW(&raw const message);
            }
            if stop.load(Ordering::Acquire) {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use windows::Win32::{
        Foundation::{LPARAM, LRESULT, WPARAM},
        System::Threading::GetCurrentThreadId,
        UI::{
            Input::KeyboardAndMouse::{
                INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_MOVE, MOUSEINPUT, SendInput,
            },
            WindowsAndMessaging::{CallNextHookEx, LLMHF_INJECTED, MSLLHOOKSTRUCT, WH_MOUSE_LL},
        },
    };

    static OBSERVER: OnceLock<Mutex<Option<mpsc::SyncSender<u32>>>> = OnceLock::new();

    unsafe extern "system" fn observe(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code >= 0 && lparam.0 != 0 {
            // SAFETY: WH_MOUSE_LL provides this structure for the callback lifetime.
            let fact = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
            if fact.flags & LLMHF_INJECTED != 0
                && let Ok(observer) = OBSERVER.get_or_init(|| Mutex::new(None)).try_lock()
                && let Some(sender) = observer.as_ref()
            {
                // SAFETY: the scalar thread ID has no ownership or pointer requirements.
                let _ = sender.try_send(unsafe { GetCurrentThreadId() });
            }
        }
        // SAFETY: preserve normal low-level hook dispatch without consuming the input.
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    #[test]
    #[ignore = "Requires an isolated interactive Windows desktop; injects two restoring mouse moves."]
    fn hook_dispatches_while_its_consumer_waits_and_can_be_reinstalled()
    -> Result<(), Box<dyn std::error::Error>> {
        // SAFETY: this reads only the test thread's scalar ID.
        let consumer = unsafe { GetCurrentThreadId() };
        for _ in 0..2 {
            let (sender, receiver) = mpsc::sync_channel(4);
            *OBSERVER
                .get_or_init(|| Mutex::new(None))
                .lock()
                .map_err(|_| "observer")? = Some(sender);
            let mut hook = HookThread::install(WH_MOUSE_LL, Some(observe))?;
            let moves = [1, -1].map(|dx| INPUT {
                r#type: INPUT_MOUSE,
                Anonymous: INPUT_0 {
                    mi: MOUSEINPUT {
                        dx,
                        dwFlags: MOUSEEVENTF_MOVE,
                        ..MOUSEINPUT::default()
                    },
                },
            });
            // SAFETY: the initialized input array is live for the synchronous call. This
            // isolated test restores relative displacement and does not change mouse settings.
            let inserted = unsafe { SendInput(&moves, i32::try_from(size_of::<INPUT>())?) };
            assert_eq!(inserted, 2);
            // Deliberately no consumer-side Windows pump: the dedicated owner must dispatch.
            assert_ne!(
                receiver.recv_timeout(std::time::Duration::from_secs(1))?,
                consumer
            );
            hook.stop();
            assert!(hook.is_finished());
            *OBSERVER
                .get_or_init(|| Mutex::new(None))
                .lock()
                .map_err(|_| "observer")? = None;
        }
        Ok(())
    }
}
