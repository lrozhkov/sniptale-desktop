//! Windows unhandled exception logging adapter.
#![cfg(windows)]
#![allow(unsafe_code)]

use std::sync::OnceLock;

use windows::Win32::System::Diagnostics::Debug::{
    EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS, LPTOP_LEVEL_EXCEPTION_FILTER,
    SetUnhandledExceptionFilter,
};

static PREVIOUS_FILTER: OnceLock<LPTOP_LEVEL_EXCEPTION_FILTER> = OnceLock::new();
static INSTALLED: OnceLock<()> = OnceLock::new();

pub(super) fn install_unhandled_exception_logger() {
    INSTALLED.get_or_init(|| {
        // SAFETY: `unhandled_exception_filter` has the required system ABI and remains valid for
        // process lifetime. The previous filter is stored so the adapter does not swallow any
        // existing process crash handling chain.
        let previous = unsafe { SetUnhandledExceptionFilter(Some(unhandled_exception_filter)) };
        let _ = PREVIOUS_FILTER.set(previous);
    });
}

unsafe extern "system" fn unhandled_exception_filter(
    exception_info: *const EXCEPTION_POINTERS,
) -> i32 {
    super::crash::log_unhandled_exception(exception_code(exception_info));
    if let Some(Some(previous)) = PREVIOUS_FILTER.get() {
        // SAFETY: Windows returned this top-level exception filter from
        // `SetUnhandledExceptionFilter`; forwarding the same callback pointer preserves the
        // previous handler contract.
        unsafe { previous(exception_info) }
    } else {
        EXCEPTION_CONTINUE_SEARCH
    }
}

fn exception_code(exception_info: *const EXCEPTION_POINTERS) -> Option<u32> {
    if exception_info.is_null() {
        return None;
    }
    // SAFETY: Windows invokes the top-level exception filter with a pointer valid for the callback
    // duration. Null is checked above, and the nested exception record pointer is checked with
    // `as_ref` before reading the code.
    let record = unsafe { (*exception_info).ExceptionRecord.as_ref()? };
    Some(record.ExceptionCode.0 as u32)
}
