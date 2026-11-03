//! Remember an external capture target while the compact tool window has focus.
#![allow(unsafe_code)]

use windows::Win32::{
    Foundation::HWND,
    Graphics::Dwm::DwmFlush,
    System::Threading::GetCurrentProcessId,
    UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId, IsIconic, IsWindow,
        IsWindowVisible, SetForegroundWindow,
    },
};

#[derive(Debug, Default)]
pub(super) struct ForegroundTarget(Option<HWND>);
impl ForegroundTarget {
    pub(super) fn observe(&mut self) {
        // SAFETY: the OS returns a borrowed handle; it is validated again before restoration.
        let window = unsafe { GetForegroundWindow() };
        if eligible(window) {
            self.0 = Some(window);
        }
    }
    pub(super) fn available(&self) -> bool {
        self.0.is_some_and(eligible)
    }
    pub(super) fn restore(&self) -> bool {
        // SAFETY: this borrows the current OS window after our presentation has been hidden.
        let current = unsafe { GetForegroundWindow() };
        if eligible(current) {
            // Prefer the OS-restored external target over an older sampled foreground window.
            return true;
        }
        let Some(window) = self.0.filter(|window| eligible(*window)) else {
            return false;
        };
        // SAFETY: restoration borrows a revalidated external window after an explicit capture request.
        unsafe { SetForegroundWindow(window).as_bool() }
    }
}
fn eligible(window: HWND) -> bool {
    let mut process = 0;
    let mut class = [0_u16; 128];
    // SAFETY: no pointer from these bounded synchronous queries survives the call.
    unsafe {
        if window.is_invalid()
            || !IsWindow(Some(window)).as_bool()
            || !IsWindowVisible(window).as_bool()
            || IsIconic(window).as_bool()
        {
            return false;
        }
        GetWindowThreadProcessId(window, Some(&raw mut process));
        if process == 0 || process == GetCurrentProcessId() {
            return false;
        }
        let count = usize::try_from(GetClassNameW(window, &mut class)).unwrap_or(0);
        if count == 0 || count >= class.len() {
            return false;
        }
        let class = String::from_utf16_lossy(&class[..count]);
        !matches!(
            class.as_str(),
            "Shell_TrayWnd" | "Shell_SecondaryTrayWnd" | "Progman" | "WorkerW"
        )
    }
}

pub(super) fn settle() -> std::io::Result<()> {
    // SAFETY: this synchronous compositor barrier retains no pointers or window handles.
    unsafe { DwmFlush() }.map_err(|_| std::io::Error::other("capture presentation did not settle"))
}
