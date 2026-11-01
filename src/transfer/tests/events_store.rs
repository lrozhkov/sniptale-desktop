use crate::{
    protocol::{
        DeliveryRoute,
        recording_events::{EventDescriptor, EventReceipt},
    },
    transfer::{
        events_store::{EventsArchive, EventsWriter},
        hashing::sha256_hex,
        trajectory_store::{TrajectoryStoreError, TrajectoryVideoBinding},
    },
};

type ProofError = Box<dyn std::error::Error>;
pub(super) const RECORDING: &str = "recording-events-fixture";
pub(super) const SOURCE: &str = "abcdef0123456789abcdef0123456789";

#[test]
fn explicit_erasure_marker_is_idempotent_and_corruption_never_authorizes_deletion()
-> Result<(), ProofError> {
    use crate::transfer::history_files::{erasure_requested, finish_erasure, request_erasure};
    let root = tempfile::tempdir()?;
    let source = fixture()?;
    let mut writer = create(root.path())?;
    writer.append(
        source["blocks"][0]["utf8"]
            .as_str()
            .ok_or("bytes")?
            .as_bytes(),
    )?;
    writer.finalize(true)?;
    drop(writer);
    let directory = root
        .path()
        .join(format!("{}-{SOURCE}", sha256_hex(RECORDING.as_bytes())));
    let head = std::fs::read(directory.join("head.json"))?;
    assert_eq!(
        finish_erasure(&directory),
        Err(TrajectoryStoreError::Invalid)
    );
    assert_eq!(std::fs::read(directory.join("head.json"))?, head);
    let marker = directory.join("erase.requested");
    std::fs::write(&marker, b"not-an-explicit-empty-request")?;
    assert_eq!(
        erasure_requested(&directory),
        Err(TrajectoryStoreError::Invalid)
    );
    assert!(request_erasure(&directory).is_err());
    assert!(finish_erasure(&directory).is_err());
    assert_eq!(std::fs::read(directory.join("head.json"))?, head);
    std::fs::write(&marker, [])?;
    request_erasure(&directory)?;
    request_erasure(&directory)?;
    assert!(erasure_requested(&directory)?);
    assert!(matches!(
        EventsArchive::read(root.path(), RECORDING, SOURCE),
        Err(TrajectoryStoreError::Retired)
    ));
    std::fs::remove_file(directory.join("blocks.bin"))?;
    std::fs::create_dir(directory.join("blocks.bin"))?;
    assert_eq!(finish_erasure(&directory), Err(TrajectoryStoreError::Io));
    assert!(erasure_requested(&directory)?);
    assert_eq!(std::fs::read(directory.join("head.json"))?, head);
    std::fs::remove_dir(directory.join("blocks.bin"))?;
    finish_erasure(&directory)?;
    finish_erasure(&directory)?;
    assert_eq!(std::fs::read(marker)?, Vec::<u8>::new());
    for name in ["head.json", "head.pending", "blocks.bin", "index.bin"] {
        assert!(!directory.join(name).exists(), "{name}");
    }
    assert!(create(root.path()).is_err());
    Ok(())
}

#[test]
fn a_failed_event_erasure_stays_unreadable_and_startup_retries_the_explicit_request()
-> Result<(), ProofError> {
    use crate::transfer::events_catalog::EventsCatalog;
    let root = tempfile::tempdir()?;
    let source = fixture()?;
    let mut writer = create(root.path())?;
    for block in source["blocks"].as_array().ok_or("blocks")? {
        writer.append(block["utf8"].as_str().ok_or("bytes")?.as_bytes())?;
    }
    writer.finalize(false)?;
    let archive = writer.into_archive();
    let directory = std::fs::read_dir(root.path())?
        .next()
        .ok_or("directory")??
        .path();
    std::fs::create_dir(directory.join("head.pending"))?;
    assert!(archive.erase().is_err());
    assert!(matches!(
        EventsArchive::read(root.path(), RECORDING, SOURCE),
        Err(TrajectoryStoreError::Retired)
    ));
    let catalog = EventsCatalog::open(root.path().to_owned());
    assert!(catalog.lookup(RECORDING, SOURCE)?.is_none());
    assert!(catalog.recover_interrupted().is_err());
    std::fs::remove_dir(directory.join("head.pending"))?;
    catalog.recover_interrupted()?;
    for name in ["head.json", "blocks.bin", "index.bin"] {
        assert!(!directory.join(name).exists(), "{name}");
    }
    assert!(create(root.path()).is_err());
    Ok(())
}

pub(super) fn fixture() -> Result<serde_json::Value, ProofError> {
    Ok(serde_json::from_str(include_str!(
        "../../../contracts/native/fixtures/recording-events-v1.json"
    ))?)
}

pub(super) fn create(root: &std::path::Path) -> Result<EventsWriter, TrajectoryStoreError> {
    EventsWriter::create(
        root,
        RECORDING,
        SOURCE,
        DeliveryRoute::DesktopDelivery {
            asset_job_id: "events-job".to_owned(),
            target_id: Some("a".repeat(64)),
            ask_after_capture: false,
        },
    )
}

#[test]
fn protected_event_store_preserves_original_bytes_and_exact_frozen_stream_identity()
-> Result<(), ProofError> {
    let fixture = fixture()?;
    let root = tempfile::tempdir()?;
    let mut writer = create(root.path())?;
    for source in fixture["blocks"].as_array().ok_or("blocks")? {
        let bytes = source["utf8"].as_str().ok_or("source bytes")?.as_bytes();
        writer.append(bytes)?;
        writer.append(bytes)?;
    }
    let expected: EventDescriptor = serde_json::from_value(fixture["descriptor"].clone())?;
    assert_eq!(writer.finalize(false)?, expected);
    drop(writer);
    let mut archive = EventsArchive::read(root.path(), RECORDING, SOURCE)?;
    let entries = archive.manifest(0, &sha256_hex(&[]))?;
    assert_eq!(serde_json::to_value(entries)?, fixture["manifest"]);
    for index in 0..3_u64 {
        assert_eq!(
            archive.read_block(index)?,
            fixture["blocks"][usize::try_from(index)?]["utf8"]
                .as_str()
                .ok_or("bytes")?
                .as_bytes()
        );
    }
    assert!(archive.manifest(1, &sha256_hex(&[])).is_err());
    assert!(archive.manifest(4, &expected.stream_sha256).is_err());
    let directory = root
        .path()
        .join(format!("{}-{SOURCE}", sha256_hex(RECORDING.as_bytes())));
    let encrypted = std::fs::read(directory.join("blocks.bin"))?;
    let field = b"\"eventCount\"";
    assert!(!encrypted.windows(field.len()).any(|window| window == field));
    assert!(
        crate::transfer::trajectory_store::TrajectoryArchive::read(root.path(), RECORDING, SOURCE)
            .is_err()
    );
    Ok(())
}

#[test]
fn video_receipt_never_releases_the_only_event_copy_and_shared_reads_cannot_mutate()
-> Result<(), ProofError> {
    let root = tempfile::tempdir()?;
    let fixture = fixture()?;
    let mut writer = create(root.path())?;
    for block in fixture["blocks"].as_array().ok_or("blocks")? {
        writer.append(block["utf8"].as_str().ok_or("bytes")?.as_bytes())?;
    }
    writer.finalize(false)?;
    let mut archive = writer.into_archive();
    archive.bind_video(TrajectoryVideoBinding {
        sha256: "a".repeat(64),
        bytes: 1000,
        duration_ms: 900,
        width: 1280,
        height: 720,
    })?;
    archive.accept_video_receipt(1000, &"a".repeat(64))?;
    assert!(!archive.both_receipts_accepted());
    drop(archive);
    let first = EventsArchive::read(root.path(), RECORDING, SOURCE)?;
    let mut second = EventsArchive::read(root.path(), RECORDING, SOURCE)?;
    assert!(matches!(
        EventsArchive::open(root.path(), RECORDING, SOURCE),
        Err(TrajectoryStoreError::Busy)
    ));
    assert!(second.accept_video_receipt(1000, &"a".repeat(64)).is_err());
    assert_ne!(second.read_block(2)?, Vec::<u8>::new());
    drop(first);
    drop(second);
    let mut archive = EventsArchive::open(root.path(), RECORDING, SOURCE)?;
    let receipt: EventReceipt = serde_json::from_value(fixture["receipt"].clone())?;
    archive.accept_event_receipt(&receipt)?;
    archive.accept_event_receipt(&receipt)?;
    assert!(archive.both_receipts_accepted());
    archive.erase()?;
    assert!(EventsArchive::read(root.path(), RECORDING, SOURCE).is_err());
    assert!(create(root.path()).is_err());
    Ok(())
}

#[test]
fn failed_head_commit_keeps_orphan_bytes_invisible_and_fences_the_writer() -> Result<(), ProofError>
{
    let root = tempfile::tempdir()?;
    let fixture = fixture()?;
    let mut writer = create(root.path())?;
    let directory = root
        .path()
        .join(format!("{}-{SOURCE}", sha256_hex(RECORDING.as_bytes())));
    std::fs::create_dir(directory.join("head.pending"))?;
    let bytes = fixture["blocks"][0]["utf8"]
        .as_str()
        .ok_or("bytes")?
        .as_bytes();
    assert!(writer.append(bytes).is_err());
    assert_eq!(writer.descriptor().total_blocks, 0);
    assert_eq!(writer.append(bytes), Err(TrajectoryStoreError::Retired));
    assert!(writer.finalize(false).is_err());
    drop(writer);
    std::fs::remove_dir(directory.join("head.pending"))?;
    let mut archive = EventsArchive::open(root.path(), RECORDING, SOURCE)?;
    assert_eq!(
        archive.manifest(0, &sha256_hex(&[]))?,
        Vec::<crate::protocol::recording_events::EventManifestEntry>::new()
    );
    assert!(archive.read_block(0).is_err());
    archive.recover_interrupted()?;
    assert_eq!(
        archive.descriptor().state,
        crate::protocol::trajectory::CursorTrajectoryState::Unavailable
    );
    Ok(())
}

#[test]
fn immutable_event_prefix_refuses_wrong_identity_conflicting_retries_and_post_finalization_append()
-> Result<(), ProofError> {
    let root = tempfile::tempdir()?;
    let fixture = fixture()?;
    let first = fixture["blocks"][0]["utf8"]
        .as_str()
        .ok_or("first")?
        .as_bytes();
    let mut writer = create(root.path())?;
    let mut wrong = fixture["blocks"][0]["value"].clone();
    wrong["recordingId"] = serde_json::json!("different-recording");
    assert_eq!(
        writer.append(&serde_json::to_vec(&wrong)?),
        Err(TrajectoryStoreError::Invalid)
    );
    assert_eq!(writer.descriptor().total_blocks, 0);
    writer.append(first)?;
    let mut changed = fixture["blocks"][0]["value"].clone();
    changed["records"][0]["clock"]["mappingUncertaintyUs"] = serde_json::json!(42);
    assert_eq!(
        writer.append(&serde_json::to_vec(&changed)?),
        Err(TrajectoryStoreError::Conflict)
    );
    let descriptor = writer.finalize(true)?;
    assert_eq!(writer.finalize(false)?, descriptor);
    assert_eq!(
        writer.append(
            fixture["blocks"][1]["utf8"]
                .as_str()
                .ok_or("second")?
                .as_bytes()
        ),
        Err(TrajectoryStoreError::Retired)
    );
    writer.append(first)?;
    assert_eq!(writer.descriptor(), &descriptor);
    Ok(())
}

#[test]
fn corrupted_event_files_are_refused_without_repairing_the_committed_head() -> Result<(), ProofError>
{
    for damage in ["short-data", "short-index", "header", "ciphertext"] {
        let root = tempfile::tempdir()?;
        let source = fixture()?;
        let mut writer = create(root.path())?;
        for block in source["blocks"].as_array().ok_or("blocks")? {
            writer.append(block["utf8"].as_str().ok_or("bytes")?.as_bytes())?;
        }
        writer.finalize(false)?;
        drop(writer);
        let path = root
            .path()
            .join(format!("{}-{SOURCE}", sha256_hex(RECORDING.as_bytes())));
        let before = sha256_hex(&std::fs::read(path.join("head.json"))?);
        if damage.starts_with("short-") {
            let file = if damage == "short-data" {
                "blocks.bin"
            } else {
                "index.bin"
            };
            std::fs::OpenOptions::new()
                .write(true)
                .open(path.join(file))?
                .set_len(0)?;
            assert!(EventsArchive::read(root.path(), RECORDING, SOURCE).is_err());
        } else {
            let mut bytes = std::fs::read(path.join("blocks.bin"))?;
            let offset = if damage == "header" { 0 } else { 36 };
            bytes[offset] ^= 1;
            std::fs::write(path.join("blocks.bin"), bytes)?;
            let mut archive = EventsArchive::read(root.path(), RECORDING, SOURCE)?;
            assert!(archive.read_block(0).is_err());
        }
        assert_eq!(sha256_hex(&std::fs::read(path.join("head.json"))?), before);
        assert!(!path.join("head.pending").exists());
    }
    Ok(())
}
