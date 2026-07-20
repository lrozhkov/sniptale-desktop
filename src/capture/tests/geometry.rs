use super::*;
#[cfg(not(windows))]
use crate::capture::screenshot;

#[test]
fn drag_selection_normalizes_negative_virtual_desktop_coordinates() -> Result<(), CoordinateError> {
    let rect = normalize_drag_selection(
        PhysicalPoint { x: 200, y: 100 },
        PhysicalPoint { x: -100, y: -50 },
        4,
    )?;

    assert_eq!(
        rect,
        PhysicalRect {
            x: -100,
            y: -50,
            width: 300,
            height: 150
        }
    );
    Ok(())
}

#[test]
fn drag_selection_rejects_near_zero_regions() {
    let error = normalize_drag_selection(
        PhysicalPoint { x: 10, y: 10 },
        PhysicalPoint { x: 12, y: 20 },
        4,
    );

    assert_eq!(error, Err(CoordinateError::SelectionTooSmall));
}

#[test]
fn region_overlay_preview_returns_current_valid_drag_rect() {
    let mut state = RegionOverlayState::new(4);
    assert_eq!(
        state.apply(OverlayEvent::DragStart(PhysicalPoint { x: 20, y: 10 })),
        OverlayOutcome::Pending
    );
    assert_eq!(
        state.apply(OverlayEvent::DragMove(PhysicalPoint { x: 40, y: 30 })),
        OverlayOutcome::Pending
    );

    assert_eq!(
        state.preview_rect(),
        Some(PhysicalRect {
            x: 20,
            y: 10,
            width: 20,
            height: 20
        })
    );
}

#[cfg(not(windows))]
#[test]
fn region_overlay_availability_fails_closed_off_windows() {
    assert_eq!(
        runtime_region_overlay_availability(),
        RegionOverlayAvailability::UnsupportedPlatform
    );
}

#[test]
fn source_local_translation_keeps_points_inside_source() {
    let source = PhysicalRect {
        x: -1920,
        y: 100,
        width: 1920,
        height: 1080,
    };

    assert_eq!(
        source_local_point(PhysicalPoint { x: -1900, y: 120 }, source),
        Some(PhysicalPoint { x: 20, y: 20 })
    );
    assert_eq!(
        source_local_point(PhysicalPoint { x: 0, y: 120 }, source),
        None
    );
}

#[test]
fn physical_rect_rejects_zero_dimensions() {
    assert_eq!(
        PhysicalRect::new(0, 0, 0, 10),
        Err(CoordinateError::EmptyRect)
    );
    assert_eq!(
        PhysicalRect::new(0, 0, 10, 0),
        Err(CoordinateError::EmptyRect)
    );
}

#[cfg(not(windows))]
#[test]
fn runtime_capture_availability_fails_closed_off_windows() {
    assert_eq!(
        runtime_capture_availability(),
        WgcRuntimeAvailability::UnsupportedPlatform
    );
}

#[cfg(not(windows))]
#[test]
fn default_screenshot_capture_runner_is_unavailable_off_windows() -> Result<(), String> {
    let temp = tempfile::tempdir().map_err(|error| error.to_string())?;
    let store = ScreenshotTransferStore::open_with_default_config(temp.path())
        .map_err(|error| format!("{error:?}"))?;

    assert!(screenshot::default_screenshot_capture_runner(store).is_none());
    Ok(())
}
