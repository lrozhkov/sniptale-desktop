//! Closed recording-events-v1 source contracts. Runtime advertisement remains separate.

#[cfg(test)]
pub(crate) const RECORDING_EVENTS_FIXTURE_SHA: &str =
    "ab6c98e72dee964de142a6b24d13d1e448a3f412cbd7543153f9cc562918ddc2";

mod attachment_validation;
#[cfg(any(windows, test))]
mod read_grant;
#[cfg(any(windows, test))]
pub(crate) use read_grant::{
    RecordingEventsReadGrant, events_read_grant_value, parse_events_read_grant,
};
mod attachments;
mod descriptor;
#[cfg(any(windows, test))]
mod labels;
pub(crate) use attachment_validation::validate_event_attachment_message;
#[cfg(any(windows, test))]
mod packing;
#[cfg(any(windows, test))]
mod records;
#[cfg(any(windows, test))]
mod stream;
#[cfg(any(windows, test))]
mod validation;

pub(crate) use attachments::{
    AppRecordingEventsAvailableMessage, AppRecordingEventsChunkMessage,
    AppRecordingEventsManifestMessage, ExtensionRecordingEventsAckMessage,
    ExtensionRecordingEventsChunkRequestMessage, ExtensionRecordingEventsManifestRequestMessage,
    NativeEventAttachmentCapabilities,
};
pub(crate) use descriptor::{
    EventAlgorithm, EventDescriptor, EventEncoding, EventFormat, EventManifestEntry, EventReceipt,
};
#[cfg(any(windows, test))]
pub(crate) use labels::valid_event_key_label;
#[cfg(any(windows, test))]
pub(crate) use packing::EventPacker;
#[cfg(any(windows, test))]
pub(crate) use stream::EventStreamValidator;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EventContractError {
    #[cfg(any(windows, test))]
    Malformed,
    #[cfg(any(windows, test))]
    Oversize,
    Invalid,
}
#[cfg(any(windows, test))]
pub(crate) use validation::{decode_event_block, validate_event_block};

#[cfg(any(windows, test))]
pub(crate) use records::{
    EventAction, EventActionData, EventActionKind, EventActionUpdate, EventBlock,
    EventBoundaryReason, EventCadence, EventCadenceEncoding, EventCadencePhase, EventClickData,
    EventClickUpdate, EventClock, EventCompletion, EventGapReason, EventGapSource, EventKeyData,
    EventKeyUpdate, EventOwnerKind, EventPoint, EventPreset, EventPrivacy, EventRecord,
    EventSegmentReason, EventSeverity, EventSignal, EventSignalData, EventSignalKind,
    EventTypingActivity, EventTypingData, EventWarningData,
};

#[cfg(test)]
pub(crate) use validation::recording_events_fixture;
