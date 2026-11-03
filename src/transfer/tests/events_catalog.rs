use super::events_store::{RECORDING, SOURCE, create, fixture};
use crate::{
    protocol::{
        DeliveryRoute,
        recording_events::{EventBlock, EventReceipt},
        trajectory::CursorTrajectoryState,
    },
    transfer::{
        browser_delivery::{BrowserDeliveryStore, BrowserMediaKind},
        events_catalog::EventsCatalog,
        events_store::{EventsArchive, EventsWriter},
        hashing::sha256_hex,
        trajectory_store::{TrajectoryStoreError, TrajectoryVideoBinding},
    },
};

type ProofError = Box<dyn std::error::Error>;

fn video() -> TrajectoryVideoBinding {
    TrajectoryVideoBinding {
        sha256: "a".repeat(64),
        bytes: 1000,
        duration_ms: 900,
        width: 1280,
        height: 720,
    }
}

fn route() -> DeliveryRoute {
    DeliveryRoute::DesktopDelivery {
        asset_job_id: "events-job".to_owned(),
        target_id: Some("a".repeat(64)),
        ask_after_capture: false,
    }
}

fn append_fixture(writer: &mut EventsWriter) -> Result<(), ProofError> {
    for block in fixture()?["blocks"].as_array().ok_or("blocks")? {
        writer.append(block["utf8"].as_str().ok_or("bytes")?.as_bytes())?;
    }
    Ok(())
}

#[test]
fn event_discovery_requires_the_exact_assignment_and_independent_video_receipt()
-> Result<(), ProofError> {
    let root = tempfile::tempdir()?;
    let history = root.path().join("history");
    let catalog = EventsCatalog::open(history.clone());
    let assignments = BrowserDeliveryStore::open(root.path().join("assignments"));
    assert!(catalog.lookup(RECORDING, SOURCE)?.is_none());
    assert_eq!(
        catalog
            .pending_after_video_ack(&"a".repeat(64), &assignments)?
            .len(),
        0
    );
    catalog.recover_interrupted()?;
    let mut writer = create(&history)?;
    append_fixture(&mut writer)?;
    writer.finalize(false)?;
    let mut archive = writer.into_archive();
    archive.bind_video(video())?;
    archive.bind_video(video())?;
    let mut wrong = video();
    wrong.width += 1;
    assert_eq!(
        archive.bind_video(wrong),
        Err(TrajectoryStoreError::Conflict)
    );
    assert_eq!(
        archive.accept_video_receipt(1001, &video().sha256),
        Err(TrajectoryStoreError::Invalid)
    );
    drop(archive);
    assert_eq!(
        catalog
            .pending_after_video_ack(&"a".repeat(64), &assignments)?
            .len(),
        0
    );
    assignments
        .select(
            RECORDING,
            BrowserMediaKind::Recording,
            &route(),
            &"a".repeat(64),
        )
        .map_err(|error| format!("{error:?}"))?;
    assert!(
        catalog
            .authorized_read(RECORDING, SOURCE, &"b".repeat(64), &assignments)
            .is_err()
    );
    let mut read = catalog.authorized_read(RECORDING, SOURCE, &"a".repeat(64), &assignments)?;
    assert!(read.bind_video(video()).is_err());
    assert!(read.recover_interrupted().is_err());
    assert_eq!(read.block_entry(0)?.sequence, 0);
    assert!(read.chunk(0, 1).is_err());
    drop(read);
    let mut archive =
        catalog.authorized_mutation(RECORDING, SOURCE, &"a".repeat(64), &assignments)?;
    archive.accept_video_receipt(video().bytes, &video().sha256)?;
    drop(archive);
    assert_eq!(
        catalog
            .pending_after_video_ack(&"b".repeat(64), &assignments)?
            .len(),
        0
    );
    let pending = catalog.pending_after_video_ack(&"a".repeat(64), &assignments)?;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].recording_id, RECORDING);
    assert!(catalog.erase_acknowledged(RECORDING, SOURCE).is_err());
    let mut archive =
        catalog.authorized_mutation(RECORDING, SOURCE, &"a".repeat(64), &assignments)?;
    let receipt: EventReceipt = serde_json::from_value(fixture()?["receipt"].clone())?;
    archive.accept_event_receipt(&receipt)?;
    drop(archive);
    assert_eq!(
        catalog
            .pending_after_video_ack(&"a".repeat(64), &assignments)?
            .len(),
        0
    );
    catalog.erase_acknowledged(RECORDING, SOURCE)?;
    assert!(catalog.lookup(RECORDING, SOURCE)?.is_none());
    Ok(())
}

#[test]
fn event_recovery_preserves_busy_writers_and_only_explicitly_seals_abandoned_sources()
-> Result<(), ProofError> {
    let root = tempfile::tempdir()?;
    let catalog = EventsCatalog::open(root.path().to_owned());
    std::fs::write(root.path().join("ignored-file"), [])?;
    std::fs::create_dir(root.path().join("uncommitted-directory"))?;
    let mut writer = create(root.path())?;
    let source = fixture()?;
    writer.append(
        source["blocks"][0]["utf8"]
            .as_str()
            .ok_or("bytes")?
            .as_bytes(),
    )?;
    catalog.recover_interrupted()?;
    assert!(!writer.descriptor().finalized);
    drop(writer);
    let read = EventsArchive::read(root.path(), RECORDING, SOURCE)?;
    assert!(!read.descriptor().finalized);
    drop(read);
    catalog.recover_interrupted()?;
    let state = catalog
        .lookup(RECORDING, SOURCE)?
        .ok_or("source")?
        .descriptor;
    assert!(state.finalized);
    assert_eq!(state.state, CursorTrajectoryState::Incomplete);
    assert_eq!(state.total_blocks, 1);
    catalog.recover_interrupted()?;
    assert_eq!(
        catalog
            .lookup(RECORDING, SOURCE)?
            .ok_or("source")?
            .descriptor,
        state
    );
    let empty = tempfile::tempdir()?;
    drop(create(empty.path())?);
    let empty_catalog = EventsCatalog::open(empty.path().to_owned());
    empty_catalog.recover_interrupted()?;
    assert_eq!(
        empty_catalog
            .lookup(RECORDING, SOURCE)?
            .ok_or("empty")?
            .descriptor
            .state,
        CursorTrajectoryState::Unavailable
    );
    Ok(())
}

#[test]
fn a_relocated_authenticated_event_head_cannot_select_another_source_directory()
-> Result<(), ProofError> {
    let root = tempfile::tempdir()?;
    let mut writer = create(root.path())?;
    append_fixture(&mut writer)?;
    drop(writer);
    let original = root
        .path()
        .join(format!("{}-{SOURCE}", sha256_hex(RECORDING.as_bytes())));
    std::fs::rename(&original, root.path().join("relocated-source"))?;
    let catalog = EventsCatalog::open(root.path().to_owned());
    assert!(matches!(
        catalog.recover_interrupted(),
        Err(TrajectoryStoreError::Invalid)
    ));
    assert!(catalog.lookup(RECORDING, SOURCE)?.is_none());
    Ok(())
}

#[test]
fn event_discovery_batches_remain_bounded_and_acknowledged_items_do_not_hide_later_sources()
-> Result<(), ProofError> {
    let root = tempfile::tempdir()?;
    let history = root.path().join("history");
    let catalog = EventsCatalog::open(history.clone());
    let assignments = BrowserDeliveryStore::open(root.path().join("assignments"));
    let source = fixture()?;
    for index in 0..20 {
        let recording = format!("catalog-recording-{index}");
        let mut writer = EventsWriter::create(&history, &recording, SOURCE, route())?;
        for block in source["blocks"].as_array().ok_or("blocks")? {
            let mut block: EventBlock = serde_json::from_value(block["value"].clone())?;
            block.recording_id.clone_from(&recording);
            writer.append(&serde_json::to_vec(&block)?)?;
        }
        writer.finalize(false)?;
        let mut archive = writer.into_archive();
        archive.bind_video(video())?;
        archive.accept_video_receipt(video().bytes, &video().sha256)?;
        assignments
            .select(
                &recording,
                BrowserMediaKind::Recording,
                &route(),
                &"a".repeat(64),
            )
            .map_err(|error| format!("{error:?}"))?;
    }
    let batch = catalog.pending_after_video_ack(&"a".repeat(64), &assignments)?;
    assert_eq!(batch.len(), 16);
    for pending in batch {
        let mut archive = catalog.authorized_mutation(
            &pending.recording_id,
            SOURCE,
            &"a".repeat(64),
            &assignments,
        )?;
        let descriptor = archive.descriptor();
        let receipt = EventReceipt {
            version: 1,
            durable: true,
            format: descriptor.format,
            state: descriptor.state,
            total_blocks: descriptor.total_blocks,
            total_records: descriptor.total_records,
            total_bytes: descriptor.total_bytes,
            stream_sha256: descriptor.stream_sha256.clone(),
        };
        archive.accept_event_receipt(&receipt)?;
    }
    assert_eq!(
        catalog
            .pending_after_video_ack(&"a".repeat(64), &assignments)?
            .len(),
        4
    );
    Ok(())
}
