//! Bounded handoff state for a tray callback received outside the polling loop.

#[derive(Debug, Default)]
pub(crate) struct TrayCallbackSlot {
    latest: Option<isize>,
}

impl TrayCallbackSlot {
    /// Coalesces callback bursts so window-message spam cannot allocate an unbounded queue.
    pub(crate) fn replace(&mut self, lparam: isize) {
        self.latest = Some(lparam);
    }

    pub(crate) fn take(&mut self) -> Option<isize> {
        self.latest.take()
    }

    /// Drops callbacks received while a modal popup is already handling the current gesture.
    pub(crate) fn clear(&mut self) {
        self.latest = None;
    }
}

#[cfg(test)]
mod tests {
    use super::TrayCallbackSlot;

    #[test]
    fn callback_bursts_keep_only_the_latest_event() {
        let mut slot = TrayCallbackSlot::default();
        slot.replace(1);
        slot.replace(2);

        assert_eq!(slot.take(), Some(2));
        assert_eq!(slot.take(), None);
    }

    #[test]
    fn clear_discards_callbacks_captured_by_a_modal_popup() {
        let mut slot = TrayCallbackSlot::default();
        slot.replace(1);

        slot.clear();

        assert_eq!(slot.take(), None);
    }
}
