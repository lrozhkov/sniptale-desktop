//! Win32 shell notification icon adapter.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::{io, mem};

use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Shell::{
                NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIIF_WARNING, NIM_ADD,
                NIM_DELETE, NIM_MODIFY, NIM_SETVERSION, NOTIFY_ICON_MESSAGE, NOTIFYICON_VERSION_4,
                NOTIFYICONDATAW, Shell_NotifyIconW,
            },
            WindowsAndMessaging::{
                HICON, ICON_BIG, ICON_SMALL, IDI_APPLICATION, LoadIconW, SendMessageW, WM_SETICON,
            },
        },
    },
    core::PCWSTR,
};

use super::state::{TrayIconVariant, TrayNotice, TrayNoticeKind, notice_content};

const IDI_SNIPTALE_MAIN: u16 = 101;
const IDI_SNIPTALE_RECORDING: u16 = 102;
const IDI_SNIPTALE_PAUSED: u16 = 103;

pub(crate) fn load_resource_icon(variant: TrayIconVariant) -> io::Result<WindowsTrayIconImage> {
    // SAFETY: querying this process module handle does not retain pointers and is synchronous.
    let module = unsafe { GetModuleHandleW(None) }.map_err(to_io_error)?;
    let resource_id = match variant {
        TrayIconVariant::Normal => IDI_SNIPTALE_MAIN,
        TrayIconVariant::Recording => IDI_SNIPTALE_RECORDING,
        TrayIconVariant::Paused => IDI_SNIPTALE_PAUSED,
    };
    // SAFETY: MAKEINTRESOURCE-style PCWSTR encodes a fixed resource id. The fallback is the
    // predefined shared system icon for repaired/legacy binaries without these resource IDs.
    let icon = unsafe {
        LoadIconW(
            Some(HINSTANCE(module.0)),
            PCWSTR(resource_id as usize as *const u16),
        )
    }
    .or_else(|_| unsafe { LoadIconW(None, IDI_APPLICATION) })
    .map_err(to_io_error)?;
    WindowsTrayIconImage::shared_resource(icon)
}

pub(crate) fn apply_main_icon_to_window(window: HWND) {
    let Ok(icon) = load_resource_icon(TrayIconVariant::Normal) else {
        return;
    };
    // SAFETY: the shared resource HICON remains valid for process life and WM_SETICON retains it.
    unsafe {
        SendMessageW(
            window,
            WM_SETICON,
            Some(windows::Win32::Foundation::WPARAM(ICON_BIG as usize)),
            Some(windows::Win32::Foundation::LPARAM(icon.handle.0 as isize)),
        );
        SendMessageW(
            window,
            WM_SETICON,
            Some(windows::Win32::Foundation::WPARAM(ICON_SMALL as usize)),
            Some(windows::Win32::Foundation::LPARAM(icon.handle.0 as isize)),
        );
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WindowsTrayWindow {
    handle: HWND,
}

impl WindowsTrayWindow {
    pub(crate) fn new(handle: HWND) -> io::Result<Self> {
        if handle.is_invalid() {
            Err(io::Error::other("invalid tray window handle"))
        } else {
            Ok(Self { handle })
        }
    }
}

#[derive(Debug)]
pub(crate) struct WindowsTrayIconImage {
    handle: HICON,
}

impl WindowsTrayIconImage {
    /// Wraps a resource icon borrowed from this process module. It is never destroyed here.
    pub(crate) fn shared_resource(handle: HICON) -> io::Result<Self> {
        if handle.is_invalid() {
            Err(io::Error::other("invalid tray icon handle"))
        } else {
            Ok(Self { handle })
        }
    }
}

#[derive(Debug)]
pub(crate) struct WindowsTrayIcon {
    window: WindowsTrayWindow,
    icon: WindowsTrayIconImage,
    id: u32,
    callback_message: u32,
    installed: bool,
}

impl WindowsTrayIcon {
    pub(crate) fn add(
        window: WindowsTrayWindow,
        icon: WindowsTrayIconImage,
        id: u32,
        callback_message: u32,
        tooltip: &str,
    ) -> io::Result<Self> {
        let mut tray_icon = Self {
            window,
            icon,
            id,
            callback_message,
            installed: false,
        };
        tray_icon.notify(NIM_ADD, tooltip, None)?;
        if let Err(error) = tray_icon.set_version() {
            let _ = tray_icon.notify(NIM_DELETE, "", None);
            return Err(error);
        }
        tray_icon.installed = true;
        Ok(tray_icon)
    }

    pub(crate) fn show_notice(&self, kind: TrayNoticeKind) -> io::Result<()> {
        self.notify(NIM_MODIFY, "Sniptale", Some((kind, notice_content(kind))))
    }

    pub(crate) fn update_tooltip(&self, tooltip: &str) -> io::Result<()> {
        self.notify(NIM_MODIFY, tooltip, None)
    }

    pub(crate) fn update_icon(
        &mut self,
        icon: WindowsTrayIconImage,
        tooltip: &str,
    ) -> io::Result<()> {
        self.notify_with_icon(NIM_MODIFY, tooltip, None, &icon)?;
        self.icon = icon;
        Ok(())
    }

    /// Explorer discards notification icons on restart. Re-add the current state atomically.
    pub(crate) fn restore_after_taskbar_restart(&mut self, tooltip: &str) -> io::Result<()> {
        self.notify(NIM_ADD, tooltip, None)?;
        self.set_version()
    }

    pub(crate) fn remove(&mut self) -> io::Result<()> {
        if !self.installed {
            return Ok(());
        }
        let result = self.notify(NIM_DELETE, "", None);
        if result.is_ok() {
            self.installed = false;
        }
        result
    }

    fn notify(
        &self,
        message: NOTIFY_ICON_MESSAGE,
        tooltip: &str,
        notice: Option<(TrayNoticeKind, TrayNotice)>,
    ) -> io::Result<()> {
        self.notify_with_icon(message, tooltip, notice, &self.icon)
    }

    fn notify_with_icon(
        &self,
        message: NOTIFY_ICON_MESSAGE,
        tooltip: &str,
        notice: Option<(TrayNoticeKind, TrayNotice)>,
        icon: &WindowsTrayIconImage,
    ) -> io::Result<()> {
        let data = self.notify_data(tooltip, notice, icon)?;
        // SAFETY: `data` is a fully initialized NOTIFYICONDATAW whose HWND/HICON are validated
        // wrappers owned by the tray adapter caller; Shell_NotifyIconW reads it during the call.
        if unsafe { Shell_NotifyIconW(message, &raw const data) }.as_bool() {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn set_version(&self) -> io::Result<()> {
        let mut data = self.notify_data("", None, &self.icon)?;
        // NIM_SETVERSION interprets this union field as a u32 version. `data` is local and fully
        // initialized before it is passed synchronously to Shell_NotifyIconW below.
        data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
        // SAFETY: the data identifies this installed icon and Shell_NotifyIconW borrows it only
        // for the duration of the call.
        if unsafe { Shell_NotifyIconW(NIM_SETVERSION, &raw const data) }.as_bool() {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn notify_data(
        &self,
        tooltip: &str,
        notice: Option<(TrayNoticeKind, TrayNotice)>,
        icon: &WindowsTrayIconImage,
    ) -> io::Result<NOTIFYICONDATAW> {
        let cb_size = u32::try_from(mem::size_of::<NOTIFYICONDATAW>())
            .map_err(|_| io::Error::other("notify icon data size exceeds u32"))?;
        let mut data = NOTIFYICONDATAW {
            cbSize: cb_size,
            hWnd: self.window.handle,
            uID: self.id,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: self.callback_message,
            hIcon: icon.handle,
            ..NOTIFYICONDATAW::default()
        };
        copy_wide_truncated(tooltip, &mut data.szTip);
        if let Some((kind, notice)) = notice {
            data.uFlags |= NIF_INFO;
            data.dwInfoFlags = match kind {
                TrayNoticeKind::ActionRejected => NIIF_WARNING,
                TrayNoticeKind::ExtensionDisconnected => NIIF_INFO,
            };
            copy_wide_truncated(notice.title, &mut data.szInfoTitle);
            copy_wide_truncated(notice.body, &mut data.szInfo);
        }
        Ok(data)
    }
}

impl Drop for WindowsTrayIcon {
    fn drop(&mut self) {
        let _ = self.remove();
    }
}

fn copy_wide_truncated<const N: usize>(value: &str, target: &mut [u16; N]) {
    let mut index = 0;
    for code_unit in value.encode_utf16().take(target.len().saturating_sub(1)) {
        target[index] = code_unit;
        index += 1;
    }
    target[index] = 0;
}

fn to_io_error(error: windows::core::Error) -> io::Error {
    io::Error::from_raw_os_error(error.code().0)
}
