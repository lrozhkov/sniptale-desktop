//! Constant-state lifecycle admission. Reference and interval lookups belong to the durable index.

use super::{
    EventBlock, EventBoundaryReason, EventContractError, EventRecord, validate_event_block,
};

#[derive(Clone, Debug, Default)]
pub(crate) struct EventStreamValidator {
    next_block: u64,
    next_record: u64,
    last_segment: Option<u64>,
    open_segment: Option<u64>,
    incomplete: bool,
    stopped: bool,
}

impl EventStreamValidator {
    pub(crate) fn accept(&mut self, block: &EventBlock) -> Result<(), EventContractError> {
        validate_event_block(block)?;
        if block.sequence != self.next_block || self.stopped {
            return Err(EventContractError::Invalid);
        }
        let mut candidate = self.clone();
        for record in &block.records {
            candidate.record(record)?;
        }
        candidate.next_block = candidate
            .next_block
            .checked_add(1)
            .ok_or(EventContractError::Invalid)?;
        *self = candidate;
        Ok(())
    }

    pub(crate) const fn incomplete(&self) -> bool {
        self.incomplete
    }
    pub(crate) const fn closed_prefix(&self) -> bool {
        self.next_record > 0 && self.open_segment.is_none()
    }

    fn record(&mut self, record: &EventRecord) -> Result<(), EventContractError> {
        let (sequence, segment, _) = record.position();
        if sequence != self.next_record || self.stopped {
            return Err(EventContractError::Invalid);
        }
        if matches!(record, EventRecord::Segment { .. }) {
            if self.open_segment.is_some()
                || self.last_segment.map_or(segment != 0, |previous| {
                    previous.checked_add(1) != Some(segment)
                })
            {
                return Err(EventContractError::Invalid);
            }
            self.open_segment = Some(segment);
            self.last_segment = Some(segment);
        } else if self.open_segment != Some(segment) {
            return Err(EventContractError::Invalid);
        }
        match record {
            EventRecord::Boundary { reason, .. } => {
                self.open_segment = None;
                self.stopped = *reason == EventBoundaryReason::Stop;
            }
            EventRecord::Gap { .. } => {
                self.incomplete = true;
                self.open_segment = None;
            }
            _ => {}
        }
        self.next_record = self
            .next_record
            .checked_add(1)
            .ok_or(EventContractError::Invalid)?;
        Ok(())
    }
}
