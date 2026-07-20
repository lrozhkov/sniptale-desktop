use std::{sync::mpsc, thread};

use super::{AssetPublishService, run_asset_publisher_worker};
use crate::{
    app::resident::{AgentComponent, WorkerCommand, WorkerEvent},
    assets::{AssetFormat, AssetJob, AssetJobId, ManualDestination},
    transfer::{
        ProtectedMediaReader,
        hashing::sha256_hex,
        protected_file::{PROTECTED_MEDIA_ALGORITHM, protect_file_in_place},
        protected_key::{new_protected_media_key, unprotect_media_key},
    },
};

#[test]
fn worker_publishes_a_large_protected_manual_screenshot_in_bounded_chunks()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("output");
    std::fs::create_dir(&output)?;
    let destination = ManualDestination::open("destination-1".to_owned(), output.clone())
        .map_err(|_| std::io::Error::other("destination"))?;
    let job_id = AssetJobId::parse("asset-job-large".to_owned())
        .map_err(|_| std::io::Error::other("job id"))?;
    let job = AssetJob::prepare(
        job_id.clone(),
        AssetFormat::Png,
        &destination,
        "capture-large.png".to_owned(),
    )
    .map_err(|_| std::io::Error::other("job"))?;
    let bytes = [b"\x89PNG\r\n\x1a\n".as_slice(), &vec![0x5a; 300 * 1024]].concat();
    let protected_path = directory.path().join("protected.png");
    std::fs::write(&protected_path, &bytes)?;
    let (protection, key) = new_protected_media_key(PROTECTED_MEDIA_ALGORITHM)
        .map_err(|_| std::io::Error::other("key"))?;
    let (_, nonce) = unprotect_media_key(&protection, PROTECTED_MEDIA_ALGORITHM)
        .map_err(|_| std::io::Error::other("unprotect"))?;
    protect_file_in_place(&protected_path, &key, nonce)
        .map_err(|_| std::io::Error::other("protect"))?;
    let reader = ProtectedMediaReader::open(
        protected_path,
        &protection,
        u64::try_from(bytes.len())?,
        sha256_hex(&bytes),
    )
    .map_err(|_| std::io::Error::other("reader"))?;
    let service = AssetPublishService::open(directory.path().join("jobs"))
        .map_err(|_| std::io::Error::other("service"))?;
    run_worker(service, job, job_id, reader)?;
    assert!(output.join("capture-large.png").is_file());
    Ok(())
}

fn run_worker(
    service: AssetPublishService,
    job: AssetJob,
    job_id: AssetJobId,
    reader: ProtectedMediaReader,
) -> Result<(), Box<dyn std::error::Error>> {
    let (events_tx, events_rx) = mpsc::sync_channel(1);
    let (commands_tx, commands_rx) = mpsc::sync_channel(2);
    let worker = thread::spawn(move || {
        run_asset_publisher_worker(
            AgentComponent::AssetPublisher,
            &events_tx,
            &commands_rx,
            Some(&service),
            None,
        );
    });
    assert_eq!(
        events_rx.recv()?,
        WorkerEvent::Started(AgentComponent::AssetPublisher)
    );
    let (prepared_tx, prepared_rx) = mpsc::sync_channel(1);
    commands_tx.send(WorkerCommand::PrepareManualAsset {
        job: Box::new(job),
        result_sender: prepared_tx,
    })?;
    prepared_rx
        .recv()?
        .map_err(|_| std::io::Error::other("prepare"))?;
    commands_tx.send(WorkerCommand::PublishManualAsset {
        job_id,
        source: Box::new(reader),
    })?;
    commands_tx.send(WorkerCommand::Stop)?;
    worker.join().map_err(|_| std::io::Error::other("worker"))?;
    Ok(())
}
