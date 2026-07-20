//! Bounded telemetry hook event queue.

use std::collections::VecDeque;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TelemetryHookQueueSnapshot<T> {
    pub(crate) events: Vec<T>,
    pub(crate) dropped_events: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TelemetryHookEventQueue<T> {
    capacity: usize,
    events: VecDeque<T>,
    dropped_events: u64,
}

impl<T> TelemetryHookEventQueue<T> {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            events: VecDeque::with_capacity(capacity),
            dropped_events: 0,
        }
    }

    pub(crate) fn push(&mut self, event: T) {
        if self.capacity == 0 || self.events.len() >= self.capacity {
            self.dropped_events = self.dropped_events.saturating_add(1);
            return;
        }
        self.events.push_back(event);
    }

    pub(crate) fn drain(&mut self) -> TelemetryHookQueueSnapshot<T> {
        let events = self.events.drain(..).collect();
        let dropped_events = self.dropped_events;
        self.dropped_events = 0;
        TelemetryHookQueueSnapshot {
            events,
            dropped_events,
        }
    }

    #[cfg(windows)]
    pub(crate) fn clear(&mut self) {
        self.events.clear();
        self.dropped_events = 0;
    }
}
