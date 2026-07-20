//! Native-host wire projections for source-neutral media lifecycle events.

use crate::{
    capture::ScreenshotCaptureEvent,
    protocol::{
        AppScreenshotStartMessage, DeliveryRoute, OperationAuthority, PROTOCOL_VERSION,
        ScreenshotMimeType,
    },
};

use super::NativeHostCommandEvent;

pub(crate) fn screenshot_command_event(
    event: ScreenshotCaptureEvent,
    extension_controller_lease_id: Option<&str>,
) -> Option<NativeHostCommandEvent> {
    match event {
        ScreenshotCaptureEvent::Completed(completion) => {
            let (OperationAuthority::ExtensionManaged { .. }, DeliveryRoute::ExtensionTransfer) =
                (&completion.authority, &completion.delivery_route)
            else {
                return None;
            };
            let controller_lease_id = extension_controller_lease_id?;
            Some(NativeHostCommandEvent::ScreenshotStart(
                AppScreenshotStartMessage {
                    protocol_version: PROTOCOL_VERSION,
                    controller_lease_id: controller_lease_id.to_owned(),
                    capture_id: completion.capture_id,
                    mode: completion.mode,
                    filename: completion.filename,
                    mime_type: ScreenshotMimeType::ImagePng,
                    total_bytes: completion.total_bytes,
                    width: completion.width,
                    height: completion.height,
                    chunk_count: completion.chunk_count,
                    sha256: completion.sha256,
                    open_editor: completion.open_editor,
                    captured_at_epoch_ms: completion.captured_at_epoch_ms,
                },
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::screenshot_command_event;
    use crate::{
        capture::{ScreenshotCaptureCompletion, ScreenshotCaptureEvent},
        protocol::{DeliveryRoute, OperationAuthority, ScreenshotMode},
    };

    #[test]
    fn manual_completion_is_never_projected_to_native_host_wire() {
        let event = ScreenshotCaptureEvent::Completed(ScreenshotCaptureCompletion {
            authority: OperationAuthority::Manual {
                profile_revision: "profile-revision-1".to_owned(),
            },
            delivery_route: DeliveryRoute::LocalAsset {
                asset_job_id: "asset-job-1".to_owned(),
            },
            capture_id: "capture-1".to_owned(),
            mode: ScreenshotMode::Screen,
            filename: "capture-1.png".to_owned(),
            width: 10,
            height: 10,
            total_bytes: 11,
            chunk_count: 1,
            sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_owned(),
            open_editor: false,
            captured_at_epoch_ms: 4_000,
        });

        assert_eq!(screenshot_command_event(event, None), None);
    }
}
