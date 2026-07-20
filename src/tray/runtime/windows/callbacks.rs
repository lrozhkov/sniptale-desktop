//! Thread-local relay from the Win32 tray callback procedure to its adapter owner.

use std::cell::RefCell;

use super::super::callback_slot::TrayCallbackSlot;

thread_local! {
    static CALLBACK: RefCell<TrayCallbackSlot> = RefCell::new(TrayCallbackSlot::default());
}

pub(super) fn enqueue(lparam: isize) {
    CALLBACK.with(|callback| callback.borrow_mut().replace(lparam));
}

pub(super) fn take() -> Option<isize> {
    CALLBACK.with(|callback| callback.borrow_mut().take())
}

pub(super) fn discard() {
    CALLBACK.with(|callback| callback.borrow_mut().clear());
}
