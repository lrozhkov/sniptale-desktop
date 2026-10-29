//! Explicit teardown ordering for persistent WGC recording resources.

pub(super) const FRAME_POLL_FAILURE_LIMIT: usize = 40;

// windows-core 0.62 projects a successful nullable WinRT interface as Error::empty (code 0).
// Preserve the older E_POINTER empty-pool representation without hiding actual device failures.
pub(super) const fn is_empty_frame_poll_hresult(code: i32) -> bool {
    code == 0 || code == 0x8000_4003_u32.cast_signed()
}

#[derive(Debug, Default)]
pub(super) struct FramePollRecovery {
    consecutive_failures: usize,
}

impl FramePollRecovery {
    pub(super) fn observe_success(&mut self) {
        self.consecutive_failures = 0;
    }

    pub(super) fn observe_failure(&mut self) -> bool {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.consecutive_failures >= FRAME_POLL_FAILURE_LIMIT
    }
}

#[derive(Debug)]
pub(super) struct RecordingCaptureOwners<S, D, A> {
    sessions: Option<S>,
    device: Option<D>,
    apartment: Option<A>,
}

impl<S, D, A> RecordingCaptureOwners<S, D, A> {
    pub(super) const fn new(sessions: S, device: D, apartment: A) -> Self {
        Self {
            sessions: Some(sessions),
            device: Some(device),
            apartment: Some(apartment),
        }
    }

    #[cfg(windows)]
    pub(super) fn sessions_and_device(&mut self) -> Option<(&mut S, &D)> {
        match (self.sessions.as_mut(), self.device.as_ref()) {
            (Some(sessions), Some(device)) => Some((sessions, device)),
            _ => None,
        }
    }
}

impl<S, D, A> Drop for RecordingCaptureOwners<S, D, A> {
    fn drop(&mut self) {
        drop(self.sessions.take());
        drop(self.device.take());
        drop(self.apartment.take());
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use super::*;

    #[derive(Debug)]
    struct DropProbe {
        name: &'static str,
        events: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Drop for DropProbe {
        fn drop(&mut self) {
            self.events.borrow_mut().push(self.name);
        }
    }

    #[test]
    fn recording_capture_owners_close_sessions_before_device_and_apartment() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let probe = |name| DropProbe {
            name,
            events: Rc::clone(&events),
        };
        let owners =
            RecordingCaptureOwners::new(probe("sessions"), probe("device"), probe("apartment"));

        drop(owners);

        assert_eq!(
            events.borrow().as_slice(),
            ["sessions", "device", "apartment"]
        );
    }

    #[test]
    fn frame_poll_recovery_is_bounded_and_resets_after_a_live_poll() {
        let mut recovery = FramePollRecovery::default();

        for _ in 1..FRAME_POLL_FAILURE_LIMIT {
            assert!(!recovery.observe_failure());
        }
        assert!(recovery.observe_failure());

        recovery.observe_success();
        assert!(!recovery.observe_failure());
    }

    #[test]
    fn empty_frame_projection_does_not_consume_device_failure_recovery_budget() {
        let mut recovery = FramePollRecovery::default();
        for code in [0, 0x8000_4003_u32.cast_signed()] {
            for _ in 0..FRAME_POLL_FAILURE_LIMIT * 2 {
                assert!(is_empty_frame_poll_hresult(code));
                recovery.observe_success();
            }
        }
        for code in [
            0x8000_4005_u32.cast_signed(),
            0x8007_0005_u32.cast_signed(),
            0x887a_0005_u32.cast_signed(),
        ] {
            assert!(!is_empty_frame_poll_hresult(code));
        }
        for _ in 1..FRAME_POLL_FAILURE_LIMIT {
            assert!(!recovery.observe_failure());
        }
        assert!(recovery.observe_failure());
    }
}
