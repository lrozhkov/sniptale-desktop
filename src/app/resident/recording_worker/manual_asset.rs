//! Manual recording artifact handoff to the local asset publisher.

use std::sync::mpsc::SyncSender;

use crate::{
    app::resident::WorkerCommand,
    assets::AssetJobId,
    recording::{ManualRecordingFinalizedEvent, RecordingTransferStore},
};

pub(super) fn enqueue_manual_recording_publish(
    asset_sender: Option<&SyncSender<WorkerCommand>>,
    event: &ManualRecordingFinalizedEvent,
) -> Result<(), ()> {
    let sender = asset_sender.ok_or(())?;
    let job_id = AssetJobId::parse(event.asset_job_id.clone()).map_err(|_| ())?;
    let store = RecordingTransferStore::open_default().map_err(|_| ())?;
    let reader = store
        .manual_asset_reader(&event.recording_id, &event.asset_job_id)
        .map_err(|_| ())?;
    sender
        .try_send(WorkerCommand::PublishManualAsset {
            delivery: Some(Box::new(super::super::delivery::DesktopMediaDelivery {
                media_id: event.recording_id.clone(),
                kind: crate::transfer::browser_delivery::BrowserMediaKind::Recording,
                route: event.delivery_route.clone(),
            })),
            job_id,
            source: Box::new(reader),
        })
        .map_err(|_| ())
}
