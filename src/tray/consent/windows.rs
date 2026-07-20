//! Win32 native consent prompt adapter for resident pipe connections.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::{ffi::OsStr, mem::size_of, os::windows::ffi::OsStrExt};

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        UI::{
            Controls::{
                TASKDIALOGCONFIG, TDCBF_NO_BUTTON, TDCBF_YES_BUTTON, TDF_CALLBACK_TIMER,
                TDM_CLICK_BUTTON, TDN_TIMER, TaskDialogIndirect,
            },
            WindowsAndMessaging::{IDNO, IDYES, SendMessageW},
        },
    },
    core::{HRESULT, PCWSTR},
};

use super::prompt::{
    ConnectionConsentPrompt, ConnectionConsentPromptRequest, ConnectionConsentPromptResult,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WindowsConnectionConsentPrompt;

impl ConnectionConsentPrompt for WindowsConnectionConsentPrompt {
    fn prompt(&self, request: &ConnectionConsentPromptRequest) -> ConnectionConsentPromptResult {
        show_connection_consent_prompt(request)
    }
}

#[derive(Debug)]
struct PromptTimerState {
    timeout_ms: u64,
    timed_out: bool,
}

fn show_connection_consent_prompt(
    request: &ConnectionConsentPromptRequest,
) -> ConnectionConsentPromptResult {
    let title = wide_null("Sniptale Native Agent");
    let instruction = wide_null("Allow this browser connection?");
    let content = wide_null(&format!(
        "Extension: {}\r\nLauncher: {} ({}, diagnostic only)\r\n\r\nAllow applies only to this connection in production builds.",
        request.extension_label, request.parent_label, request.parent_category
    ));
    let mut timer = PromptTimerState {
        timeout_ms: request.timeout_ms,
        timed_out: false,
    };
    let config = TASKDIALOGCONFIG {
        cbSize: u32::try_from(size_of::<TASKDIALOGCONFIG>()).unwrap_or(u32::MAX),
        dwFlags: TDF_CALLBACK_TIMER,
        dwCommonButtons: TDCBF_YES_BUTTON | TDCBF_NO_BUTTON,
        pszWindowTitle: PCWSTR(title.as_ptr()),
        pszMainInstruction: PCWSTR(instruction.as_ptr()),
        pszContent: PCWSTR(content.as_ptr()),
        nDefaultButton: IDNO.0,
        pfCallback: Some(consent_prompt_callback),
        lpCallbackData: std::ptr::from_mut(&mut timer) as isize,
        ..TASKDIALOGCONFIG::default()
    };
    let mut button = IDNO.0;
    // SAFETY: config and all referenced UTF-16/timer storage remain alive for the synchronous
    // TaskDialogIndirect call. The callback only accesses lpCallbackData on this same UI thread.
    let result = unsafe { TaskDialogIndirect(&config, Some(&mut button), None, None) };
    if result.is_err() {
        ConnectionConsentPromptResult::UiUnavailable
    } else if timer.timed_out {
        ConnectionConsentPromptResult::Timeout
    } else if button == IDYES.0 {
        ConnectionConsentPromptResult::Allow
    } else {
        ConnectionConsentPromptResult::Deny
    }
}

unsafe extern "system" fn consent_prompt_callback(
    window: HWND,
    message: windows::Win32::UI::Controls::TASKDIALOG_NOTIFICATIONS,
    wparam: WPARAM,
    _lparam: LPARAM,
    callback_data: isize,
) -> HRESULT {
    if message == TDN_TIMER && callback_data != 0 {
        // SAFETY: callback_data points to PromptTimerState owned by the synchronous caller for the
        // full TaskDialogIndirect lifetime; the callback executes on that same thread.
        let timer = unsafe { &mut *(callback_data as *mut PromptTimerState) };
        if !timer.timed_out && u64::try_from(wparam.0).unwrap_or(u64::MAX) >= timer.timeout_ms {
            timer.timed_out = true;
            // SAFETY: Task Dialog owns this HWND during the callback. Clicking No closes the modal
            // dialog synchronously and prevents any approval after the owned deadline.
            unsafe {
                SendMessageW(
                    window,
                    u32::try_from(TDM_CLICK_BUTTON.0).unwrap_or_default(),
                    Some(WPARAM(usize::try_from(IDNO.0).unwrap_or_default())),
                    Some(LPARAM(0)),
                )
            };
        }
    }
    HRESULT(0)
}

fn wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain([0]).collect()
}
