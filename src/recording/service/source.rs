//! Recording command source support policy.

use crate::protocol::{NativeRecordingSource, VideoMode};
use crate::transfer::identity::derived_media_id;

pub(super) fn recording_id_for_operation(operation_id: &str) -> String {
    derived_media_id("recording", operation_id)
}

pub(super) fn recording_source_supported(source: &NativeRecordingSource) -> bool {
    match source.mode {
        VideoMode::Screen | VideoMode::ActiveWindow => true,
        VideoMode::Region => source
            .region
            .as_ref()
            .is_some_and(|region| region.width > 0 && region.height > 0),
    }
}
