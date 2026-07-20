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
) {
    let Some(sender) = asset_sender else {
        return;
    };
    let Ok(job_id) = AssetJobId::parse(event.asset_job_id.clone()) else {
        return;
    };
    let Ok(store) = RecordingTransferStore::open_default() else {
        return;
    };
    let Ok(reader) = store.manual_asset_reader(&event.recording_id, &event.asset_job_id) else {
        return;
    };
    let _ = sender.try_send(WorkerCommand::PublishManualAsset {
        job_id,
        source: Box::new(reader),
    });
}
