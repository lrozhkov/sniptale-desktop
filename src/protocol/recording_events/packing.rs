//! Greedy bounded source-block construction; delivery and timers never add seals.

use super::{
    EventBlock, EventContractError, EventRecord, decode_event_block, validate_event_block,
};

#[derive(Debug)]
pub(crate) struct EventPacker {
    recording_id: String,
    event_stream_id: String,
    sequence: u64,
    next_record: u64,
    pending: Vec<Vec<u8>>,
    pending_bytes: usize,
    start: u64,
    end: u64,
    finished: bool,
}

impl EventPacker {
    pub(crate) fn new(recording_id: String, event_stream_id: String) -> Self {
        Self {
            recording_id,
            event_stream_id,
            sequence: 0,
            next_record: 0,
            pending: Vec::new(),
            pending_bytes: 0,
            start: u64::MAX,
            end: 0,
            finished: false,
        }
    }

    pub(crate) fn push(&mut self, record: EventRecord) -> Result<Vec<Vec<u8>>, EventContractError> {
        if self.finished || record.position().0 != self.next_record {
            return Err(EventContractError::Invalid);
        }
        let start = record.position().2;
        let end = match &record {
            EventRecord::Gap {
                to_time_us: Some(end),
                ..
            } => start.max(*end),
            _ => start,
        };
        let mut single = self.header(start, end);
        let barrier = matches!(
            &record,
            EventRecord::Boundary { .. } | EventRecord::Gap { .. }
        );
        let bytes = serde_json::to_vec(&record).map_err(|_| EventContractError::Malformed)?;
        single.records.push(record);
        validate_event_block(&single)?;
        if self.header_bytes(start, end)?.len() + bytes.len() > 65536 {
            return Err(EventContractError::Oversize);
        }
        let candidate_size = self
            .header_bytes(self.start.min(start), self.end.max(end))?
            .len()
            + self.pending_bytes
            + bytes.len()
            + self.pending.len();
        let mut sealed = Vec::new();
        if (self.pending.len() == 512 || candidate_size > 65536)
            && let Some(block) = self.seal()?
        {
            sealed.push(block);
        }
        self.start = self.start.min(start);
        self.end = self.end.max(end);
        self.pending_bytes += bytes.len();
        self.pending.push(bytes);
        self.next_record = self
            .next_record
            .checked_add(1)
            .ok_or(EventContractError::Invalid)?;
        if barrier && let Some(block) = self.seal()? {
            sealed.push(block);
        }
        Ok(sealed)
    }

    pub(crate) fn finish(&mut self) -> Result<Vec<Vec<u8>>, EventContractError> {
        let result = self.seal()?.into_iter().collect();
        self.finished = true;
        Ok(result)
    }

    #[cfg(test)]
    pub(crate) fn pending_records(&self) -> usize {
        self.pending.len()
    }

    fn seal(&mut self) -> Result<Option<Vec<u8>>, EventContractError> {
        if self.pending.is_empty() {
            return Ok(None);
        }
        let mut bytes = self.header_bytes(self.start, self.end)?;
        bytes.truncate(bytes.len() - 2);
        for (index, record) in self.pending.iter().enumerate() {
            if index != 0 {
                bytes.push(b',');
            }
            bytes.extend_from_slice(record);
        }
        bytes.extend_from_slice(b"]}");
        // One complete admission per seal, without repeatedly serializing the growing prefix.
        decode_event_block(&bytes)?;
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or(EventContractError::Invalid)?;
        self.pending.clear();
        self.pending_bytes = 0;
        self.start = u64::MAX;
        self.end = 0;
        Ok(Some(bytes))
    }

    fn header(&self, start: u64, end: u64) -> EventBlock {
        EventBlock {
            format_version: 1,
            recording_id: self.recording_id.clone(),
            event_stream_id: self.event_stream_id.clone(),
            sequence: self.sequence,
            start_time_us: start,
            end_time_us: end,
            records: Vec::new(),
        }
    }

    fn header_bytes(&self, start: u64, end: u64) -> Result<Vec<u8>, EventContractError> {
        serde_json::to_vec(&self.header(start, end)).map_err(|_| EventContractError::Malformed)
    }
}
