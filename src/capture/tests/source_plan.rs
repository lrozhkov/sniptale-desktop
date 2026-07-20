use super::*;

#[test]
fn screen_source_uses_monitor_containing_cursor() -> Result<(), SourceResolveError> {
    let source = resolve_source(CaptureSourceMode::Screen, &source_context())?;

    assert_eq!(source.kind, ResolvedSourceKind::Screen);
    assert_eq!(source.monitor_id, Some("left".to_owned()));
    assert_eq!(source.bounds.x, -1920);
    Ok(())
}

#[test]
fn all_screens_source_uses_virtual_desktop_union() -> Result<(), SourceResolveError> {
    let source = resolve_source(CaptureSourceMode::AllScreens, &source_context())?;

    assert_eq!(source.kind, ResolvedSourceKind::AllScreens);
    assert_eq!(
        source.bounds,
        PhysicalRect {
            x: -1920,
            y: 0,
            width: 3840,
            height: 1080,
        }
    );
    Ok(())
}

#[test]
fn region_capture_uses_only_intersecting_monitor_bounds() -> Result<(), SourceResolveError> {
    let context = source_context();
    let primary_only = region_capture_bounds(
        &context.monitors,
        PhysicalRect {
            x: 100,
            y: 100,
            width: 400,
            height: 300,
        },
    )?;
    assert_eq!(primary_only, context.monitors[1].bounds);

    let cross_monitor = region_capture_bounds(
        &context.monitors,
        PhysicalRect {
            x: -100,
            y: 100,
            width: 200,
            height: 300,
        },
    )?;
    assert_eq!(
        cross_monitor,
        PhysicalRect {
            x: -1920,
            y: 0,
            width: 3840,
            height: 1080,
        }
    );
    Ok(())
}

#[test]
fn region_capture_rejects_a_region_that_intersects_no_monitor() {
    let context = source_context();
    assert_eq!(
        region_capture_bounds(
            &context.monitors,
            PhysicalRect {
                x: 10_000,
                y: 10_000,
                width: 100,
                height: 100,
            },
        ),
        Err(SourceResolveError::RegionOutsideVirtualDesktop)
    );
}

#[test]
fn active_window_source_reports_specific_uncapturable_states() {
    let mut context = source_context();
    for (state, expected) in [
        (
            WindowCaptureState::Minimized,
            SourceResolveError::ActiveWindowMinimized,
        ),
        (
            WindowCaptureState::Cloaked,
            SourceResolveError::ActiveWindowCloaked,
        ),
        (
            WindowCaptureState::Protected,
            SourceResolveError::ActiveWindowProtected,
        ),
        (
            WindowCaptureState::Inaccessible,
            SourceResolveError::ActiveWindowUncapturable,
        ),
    ] {
        context.active_window = Some(WindowSnapshot {
            bounds: PhysicalRect {
                x: 100,
                y: 100,
                width: 800,
                height: 600,
            },
            capture_state: state,
        });

        assert_eq!(
            resolve_source(CaptureSourceMode::ActiveWindow, &context),
            Err(expected)
        );
    }
}

#[test]
fn active_window_source_fails_when_outside_desktop() {
    let mut context = source_context();

    context.active_window = Some(WindowSnapshot {
        bounds: PhysicalRect {
            x: 5000,
            y: 0,
            width: 100,
            height: 100,
        },
        capture_state: WindowCaptureState::Capturable,
    });
    assert_eq!(
        resolve_source(CaptureSourceMode::ActiveWindow, &context),
        Err(SourceResolveError::ActiveWindowUncapturable)
    );
}

#[test]
fn overlay_confirms_valid_region_and_releases_on_display_change() {
    let mut overlay = RegionOverlayState::new(4);
    assert_eq!(
        overlay.apply(OverlayEvent::DragStart(PhysicalPoint { x: 40, y: 40 })),
        OverlayOutcome::Pending
    );
    overlay.apply(OverlayEvent::DragMove(PhysicalPoint { x: 10, y: 20 }));
    assert_eq!(
        overlay.apply(OverlayEvent::Confirm),
        OverlayOutcome::Confirmed(PhysicalRect {
            x: 10,
            y: 20,
            width: 30,
            height: 20,
        })
    );
    assert!(overlay.released());

    let mut overlay = RegionOverlayState::new(4);
    assert_eq!(
        overlay.apply(OverlayEvent::DisplayChanged),
        OverlayOutcome::Cancelled(OverlayCancelReason::DisplayChanged)
    );
    assert!(overlay.released());
}

#[test]
fn screenshot_plan_requires_region_freeze_and_rejects_protected_content() -> Result<(), &'static str>
{
    let Ok(source) = resolve_source(CaptureSourceMode::Region, &source_context()) else {
        return Err("region source should resolve");
    };
    let Ok(plan) = plan_screenshot(
        source.clone(),
        WgcSourcePolicy {
            protected_content: false,
        },
        10_000_000,
    ) else {
        return Err("region screenshot plan should be valid");
    };
    assert!(plan.requires_freeze_overlay);

    assert_eq!(
        plan_screenshot(
            source,
            WgcSourcePolicy {
                protected_content: true,
            },
            10_000_000,
        ),
        Err(ScreenshotPlanError::Frame(WgcPlanError::ProtectedContent))
    );
    Ok(())
}

#[test]
fn bgra_frame_accepts_valid_stride_with_padding() -> Result<(), BgraFrameError> {
    let pixels = [0; 24];
    let frame = BgraFrame::new(2, 2, 12, &pixels)?;

    assert_eq!(frame.width(), 2);
    assert_eq!(frame.height(), 2);
    assert_eq!(frame.stride(), 12);
    assert_eq!(frame.pixels().len(), 24);
    Ok(())
}

#[test]
fn bgra_frame_rejects_invalid_geometry_before_wic() {
    assert_eq!(
        BgraFrame::new(0, 1, 4, &[0; 4]),
        Err(BgraFrameError::EmptyDimensions)
    );
    assert_eq!(
        BgraFrame::new(2, 1, 7, &[0; 8]),
        Err(BgraFrameError::StrideTooSmall)
    );
    assert_eq!(
        BgraFrame::new(2, 2, 8, &[0; 15]),
        Err(BgraFrameError::BufferTooSmall)
    );
    assert_eq!(
        BgraFrame::new(u32::MAX, 1, u32::MAX, &[0; 8]),
        Err(BgraFrameError::GeometryTooLarge)
    );
}

#[test]
fn raw_screenshot_frame_bounds_reject_oversize_buffers_before_encoding() {
    assert!(validate_raw_frame_bounds(3840, 2160, 3840 * 4).is_ok());
    assert_eq!(
        validate_raw_frame_bounds(1, 1, 3),
        Err(ScreenshotCaptureError::RawFrameTooLarge)
    );
    assert_eq!(
        validate_raw_frame_bounds(16_384, 16_384, 16_384 * 4),
        Err(ScreenshotCaptureError::RawFrameTooLarge)
    );
}

#[test]
fn all_screens_compositor_places_frames_in_virtual_desktop_offsets() -> Result<(), String> {
    let bounds = PhysicalRect {
        x: -1,
        y: 0,
        width: 2,
        height: 2,
    };
    let left = captured_frame(
        PhysicalRect {
            x: -1,
            y: 0,
            width: 1,
            height: 2,
        },
        &[1, 0, 0, 255, 2, 0, 0, 255],
    );
    let right = captured_frame(
        PhysicalRect {
            x: 0,
            y: 0,
            width: 1,
            height: 2,
        },
        &[3, 0, 0, 255, 4, 0, 0, 255],
    );

    let frame =
        composite_bgra_frames(&[left, right], bounds).map_err(|error| format!("{error:?}"))?;

    assert_eq!(frame.bounds, bounds);
    assert_eq!(frame.width, 2);
    assert_eq!(frame.height, 2);
    assert_eq!(frame.stride, 8);
    assert_eq!(
        frame.bgra_pixels,
        vec![1, 0, 0, 255, 3, 0, 0, 255, 2, 0, 0, 255, 4, 0, 0, 255]
    );
    Ok(())
}

#[test]
fn all_screens_compositor_rejects_frames_outside_virtual_desktop() {
    let bounds = PhysicalRect {
        x: 0,
        y: 0,
        width: 2,
        height: 2,
    };
    let outside = captured_frame(
        PhysicalRect {
            x: 2,
            y: 0,
            width: 1,
            height: 2,
        },
        &[1, 0, 0, 255, 2, 0, 0, 255],
    );

    assert_eq!(
        composite_bgra_frames(&[outside], bounds),
        Err(ScreenshotCaptureError::CaptureUnavailable)
    );
}

#[test]
fn bgra_crop_extracts_region_from_virtual_desktop_frame() -> Result<(), String> {
    let frame = captured_frame(
        PhysicalRect {
            x: -1,
            y: 10,
            width: 3,
            height: 2,
        },
        &[
            1, 0, 0, 255, 2, 0, 0, 255, 3, 0, 0, 255, 4, 0, 0, 255, 5, 0, 0, 255, 6, 0, 0, 255,
        ],
    );

    let crop = crop_bgra_frame(
        &frame,
        PhysicalRect {
            x: 0,
            y: 10,
            width: 2,
            height: 2,
        },
    )
    .map_err(|error| format!("{error:?}"))?;

    assert_eq!(
        crop.bgra_pixels,
        vec![2, 0, 0, 255, 3, 0, 0, 255, 5, 0, 0, 255, 6, 0, 0, 255]
    );
    assert_eq!(
        crop.bounds,
        PhysicalRect {
            x: 0,
            y: 10,
            width: 2,
            height: 2,
        }
    );
    Ok(())
}
