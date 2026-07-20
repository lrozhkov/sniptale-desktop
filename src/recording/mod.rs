//! Recording state, encoder ownership, quality policy, and temporary media state.

mod availability;
#[cfg(any(windows, test))]
mod disk_guard;
#[cfg(any(windows, test))]
mod encoder;
mod host_runtime;
#[cfg(any(windows, test))]
mod progress;
#[cfg(any(windows, test))]
mod quality;
mod recovery;
#[cfg(any(windows, test))]
mod runtime;
#[cfg(any(windows, test))]
mod service;
mod service_contract;
#[cfg(any(windows, test))]
mod session;
#[cfg(any(windows, test))]
mod temp_media;
#[cfg(test)]
mod tests;
#[cfg(any(windows, test))]
mod transfer_store;

#[cfg(test)]
pub(crate) use availability::recording_runtime_availability_from_facts;
pub(crate) use availability::{RecordingRuntimeAvailability, recording_runtime_availability};
pub(crate) use host_runtime::{
    MediaFoundationAvailability, VideoEncoderAvailability, media_foundation_availability,
    video_encoder_availability,
};
pub(crate) use recovery::PendingRecordingTransfer;
#[cfg(any(windows, test))]
pub(crate) use runtime::RecordingRuntimeError;
#[cfg(windows)]
pub(crate) use runtime::default_recording_runner_with_telemetry;
pub(crate) use service_contract::RecordingStartPolicy;
#[cfg(any(windows, test))]
pub(crate) use service_contract::{
    ManualRecordingFinalizedEvent, RecordingCommandEvent, RecordingControlRequest,
    RecordingDeliverySession, RecordingProgressEvent, RecordingRunner, RecordingStartRequest,
    RecordingStartedEvent, RecordingStoppedEvent,
};
#[cfg(any(windows, test))]
pub(crate) use transfer_store::{RecordingTransferStore, RecordingTransferStoreError};
