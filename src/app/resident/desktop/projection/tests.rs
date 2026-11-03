use super::*;
use crate::tray::desktop::model::{Availability, CaptureTargets, Permission};

pub(crate) fn capabilities() -> CaptureCapabilities {
    let available = Availability {
        supported: true,
        permission: Permission::NotRequired,
        source_available: true,
    };
    CaptureCapabilities {
        screenshot: available,
        video: available,
        targets: CaptureTargets {
            region: available,
            display: available,
            window: available,
        },
    }
}

#[test]
fn completion_requires_matching_successful_publication() -> Result<(), String> {
    let projection = CaptureProjection::default();
    projection.begin(
        "operation",
        Some("job"),
        CaptureKind::Screenshot,
        CapturePhase::Selecting,
        None,
    );
    projection.publication("job", true);
    assert_eq!(snapshot(&projection)?.outcome, None);
    projection.advance("operation", CapturePhase::Saving);
    projection.publication("foreign-job", true);
    assert_eq!(snapshot(&projection)?.phase, CapturePhase::Saving);
    projection.publication("job", false);
    assert_eq!(
        snapshot(&projection)?.outcome,
        Some(CaptureOutcome::SaveFailed)
    );
    let failed = snapshot(&projection)?;
    projection.publication("job", true);
    assert_eq!(snapshot(&projection)?, failed);
    Ok(())
}

#[test]
fn stale_events_cannot_replace_a_new_operation_or_terminal_receipt() -> Result<(), String> {
    let projection = CaptureProjection::default();
    projection.begin(
        "old",
        Some("old-job"),
        CaptureKind::Screenshot,
        CapturePhase::Starting,
        None,
    );
    projection.begin(
        "new",
        Some("new-job"),
        CaptureKind::Screenshot,
        CapturePhase::Starting,
        None,
    );
    projection.fail("old");
    projection.advance("old", CapturePhase::Saving);
    projection.publication("old-job", true);
    assert_eq!(snapshot(&projection)?.phase, CapturePhase::Starting);
    projection.advance("new", CapturePhase::Saving);
    projection.publication("new-job", true);
    let receipt = snapshot(&projection)?;
    assert_eq!(receipt.outcome, Some(CaptureOutcome::Saved));
    projection.fail("new");
    projection.advance("new", CapturePhase::Starting);
    projection.begin(
        "new",
        Some("new-job"),
        CaptureKind::Screenshot,
        CapturePhase::Starting,
        None,
    );
    assert_eq!(snapshot(&projection)?, receipt);
    Ok(())
}

#[test]
fn recording_progress_is_correlated_and_cannot_reopen_finalization() -> Result<(), String> {
    let projection = CaptureProjection::default();
    projection.begin(
        "op",
        Some("job"),
        CaptureKind::Video,
        CapturePhase::Starting,
        Some("recording"),
    );
    projection.recording("other", CapturePhase::Recording, 50);
    assert_eq!(snapshot(&projection)?.phase, CapturePhase::Starting);
    projection.recording("recording", CapturePhase::Recording, 100);
    projection.recording("recording", CapturePhase::Paused, 200);
    projection.recording("recording", CapturePhase::Recording, 100);
    assert_eq!(snapshot(&projection)?.phase, CapturePhase::Paused);
    projection.recording_finalized("recording");
    projection.recording("recording", CapturePhase::Recording, 300);
    assert_eq!(snapshot(&projection)?.phase, CapturePhase::Saving);
    projection.publication("job", true);
    assert_eq!(snapshot(&projection)?.phase, CapturePhase::Complete);
    Ok(())
}

#[test]
fn cancellation_is_terminal_and_clone_observers_recover_the_same_state() -> Result<(), String> {
    let projection = CaptureProjection::default();
    let observer = projection.clone();
    projection.begin(
        "op",
        Some("job"),
        CaptureKind::Screenshot,
        CapturePhase::Selecting,
        None,
    );
    projection.cancel("op");
    assert_eq!(
        snapshot(&observer)?.outcome,
        Some(CaptureOutcome::Cancelled)
    );
    projection.fail("op");
    assert_eq!(snapshot(&observer)?.phase, CapturePhase::Complete);
    Ok(())
}

fn snapshot(projection: &CaptureProjection) -> Result<CaptureSnapshot, String> {
    projection
        .snapshot(
            capabilities(),
            crate::tray::desktop::model::CaptureQuality::Balanced,
        )
        .map_err(|error| format!("{error:?}"))
}

#[test]
fn operation_epoch_binds_old_controls_without_expiring_on_progress() {
    let projection = CaptureProjection::default();
    assert!(projection.epoch_matches(0));
    projection.begin(
        "first",
        Some("job-one"),
        CaptureKind::Video,
        CapturePhase::Starting,
        Some("recording-one"),
    );
    assert!(!projection.epoch_matches(0));
    assert!(projection.epoch_matches(1));
    projection.recording("recording-one", CapturePhase::Recording, 100);
    projection.recording("recording-one", CapturePhase::Paused, 200);
    assert!(projection.epoch_matches(1));
    projection.recording_discarded("recording-one");
    projection.begin(
        "second",
        Some("job-two"),
        CaptureKind::Video,
        CapturePhase::Starting,
        Some("recording-two"),
    );
    assert!(!projection.epoch_matches(1));
    assert!(projection.epoch_matches(2));
}

#[test]
fn parallel_screenshots_and_rejected_starts_do_not_replace_active_recording_controls()
-> Result<(), String> {
    let projection = CaptureProjection::default();
    projection.begin(
        "video-op",
        Some("video-job"),
        CaptureKind::Video,
        CapturePhase::Starting,
        Some("video"),
    );
    projection.recording("video", CapturePhase::Recording, 1000);
    let recording = snapshot(&projection)?;
    projection.begin(
        "screenshot-op",
        Some("image-job"),
        CaptureKind::Screenshot,
        CapturePhase::Starting,
        None,
    );
    projection.advance("screenshot-op", CapturePhase::Saving);
    projection.publication("image-job", true);
    projection.begin(
        "rejected-video",
        Some("rejected-job"),
        CaptureKind::Video,
        CapturePhase::Starting,
        Some("rejected"),
    );
    projection.reject_pending("rejected-video");
    projection.reject_pending("video-op");
    assert_eq!(snapshot(&projection)?, recording);
    projection.recording_finalized("video");
    projection.begin(
        "next-video",
        Some("next-job"),
        CaptureKind::Video,
        CapturePhase::Starting,
        Some("next"),
    );
    projection.publication("video-job", true);
    assert_eq!(snapshot(&projection)?.phase, CapturePhase::Starting);
    assert!(projection.epoch_matches(2));
    Ok(())
}

#[test]
fn finalized_transfer_media_does_not_claim_durable_publication() -> Result<(), String> {
    let projection = CaptureProjection::default();
    projection.begin(
        "browser-op",
        None,
        CaptureKind::Video,
        CapturePhase::Starting,
        Some("recording"),
    );
    projection.recording("recording", CapturePhase::Recording, 1000);
    projection.recording_completed("recording");
    assert_eq!(snapshot(&projection)?.phase, CapturePhase::Complete);
    assert_eq!(snapshot(&projection)?.outcome, None);
    projection.publication("unrelated-job", true);
    assert_eq!(snapshot(&projection)?.outcome, None);
    Ok(())
}
