//! Win32 source snapshot and WGC item creation.
#![allow(unsafe_code)]

mod desktop;

use windows::{
    Graphics::Capture::GraphicsCaptureItem,
    Win32::{
        Foundation::HWND, Graphics::Gdi::HMONITOR,
        System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop,
    },
    core::factory,
};

use crate::protocol::{NativeRecordingSource, ScreenshotMode, VideoMode};

use super::super::super::{
    coordinates::{PhysicalPoint, PhysicalRect},
    screenshot::ScreenshotCaptureError,
    sources::{
        CaptureSourceMode, MonitorSnapshot, SourceResolveContext, WindowSnapshot,
        region_capture_bounds, resolve_source,
    },
};

use self::desktop::{active_window, cursor_position, enumerate_monitors};

#[derive(Clone, Copy)]
enum CaptureItemSource {
    Monitor(HMONITOR),
    Window(HWND),
}

#[derive(Clone)]
pub(super) struct CaptureTargets {
    pub(super) capture_bounds: PhysicalRect,
    pub(super) output_bounds: PhysicalRect,
    pub(super) items: Vec<CaptureTarget>,
}

#[derive(Clone, Copy)]
pub(super) struct CaptureTarget {
    source: CaptureItemSource,
    pub(super) bounds: PhysicalRect,
}

impl CaptureTargets {
    pub(super) fn resolve(
        mode: ScreenshotMode,
        selected_region: Option<PhysicalRect>,
    ) -> Result<Self, ScreenshotCaptureError> {
        match mode {
            ScreenshotMode::Screen => NativeSourceSnapshot::current()?.screen_targets(),
            ScreenshotMode::ActiveWindow => {
                NativeSourceSnapshot::current()?.active_window_targets()
            }
            ScreenshotMode::AllScreens => NativeSourceSnapshot::current()?.all_screens_targets(),
            ScreenshotMode::Region => NativeSourceSnapshot::current()?
                .region_targets(selected_region.ok_or(ScreenshotCaptureError::UnsupportedMode)?),
        }
    }
}

pub(crate) fn recording_source_bounds(
    source: &NativeRecordingSource,
) -> Result<PhysicalRect, ScreenshotCaptureError> {
    recording_targets(source).map(|targets| targets.output_bounds)
}

pub(super) fn recording_targets(
    source: &NativeRecordingSource,
) -> Result<CaptureTargets, ScreenshotCaptureError> {
    let mode = recording_mode(source.mode.clone());
    let region = source
        .region
        .as_ref()
        .map(|region| {
            let x = i32::try_from(region.x).map_err(|_| ScreenshotCaptureError::UnsupportedMode)?;
            let y = i32::try_from(region.y).map_err(|_| ScreenshotCaptureError::UnsupportedMode)?;
            PhysicalRect::new(x, y, region.width, region.height)
                .map_err(|_| ScreenshotCaptureError::UnsupportedMode)
        })
        .transpose()
        .map_err(|_| ScreenshotCaptureError::UnsupportedMode)?;
    CaptureTargets::resolve(mode, region)
}

impl CaptureTarget {
    pub(super) fn create_item(&self) -> Result<GraphicsCaptureItem, ScreenshotCaptureError> {
        let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
            .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
        // SAFETY: The handles come from user32 monitor/window queries in this adapter and are used
        // immediately to create WinRT capture items; raw handles never leave the capture owner.
        unsafe {
            match self.source {
                CaptureItemSource::Monitor(monitor) => interop.CreateForMonitor(monitor),
                CaptureItemSource::Window(window) => interop.CreateForWindow(window),
            }
        }
        .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)
    }
}

const fn recording_mode(mode: VideoMode) -> ScreenshotMode {
    match mode {
        VideoMode::Screen => ScreenshotMode::Screen,
        VideoMode::ActiveWindow => ScreenshotMode::ActiveWindow,
        VideoMode::Region => ScreenshotMode::Region,
    }
}

struct NativeMonitor {
    handle: HMONITOR,
    snapshot: MonitorSnapshot,
}

struct NativeSourceSnapshot {
    monitors: Vec<NativeMonitor>,
    cursor: Option<PhysicalPoint>,
    active_window: Option<NativeWindow>,
}

struct NativeWindow {
    handle: HWND,
    snapshot: WindowSnapshot,
}

impl NativeSourceSnapshot {
    fn current() -> Result<Self, ScreenshotCaptureError> {
        let monitors = enumerate_monitors()?;
        Ok(Self {
            monitors,
            cursor: cursor_position(),
            active_window: active_window(),
        })
    }

    fn screen_targets(&self) -> Result<CaptureTargets, ScreenshotCaptureError> {
        let context = self.context(None, None);
        let source = resolve_source(CaptureSourceMode::Screen, &context)
            .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
        let monitor_id = source
            .monitor_id
            .as_deref()
            .ok_or(ScreenshotCaptureError::CaptureUnavailable)?;
        let monitor = self
            .monitors
            .iter()
            .find(|monitor| monitor.snapshot.id == monitor_id)
            .ok_or(ScreenshotCaptureError::CaptureUnavailable)?;
        Ok(CaptureTargets {
            capture_bounds: source.bounds,
            output_bounds: source.bounds,
            items: vec![CaptureTarget {
                source: CaptureItemSource::Monitor(monitor.handle),
                bounds: source.bounds,
            }],
        })
    }

    fn active_window_targets(&self) -> Result<CaptureTargets, ScreenshotCaptureError> {
        let window = self
            .active_window
            .as_ref()
            .ok_or(ScreenshotCaptureError::CaptureUnavailable)?;
        let context = self.context(Some(window.snapshot.clone()), None);
        let source = resolve_source(CaptureSourceMode::ActiveWindow, &context)
            .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
        Ok(CaptureTargets {
            capture_bounds: source.bounds,
            output_bounds: source.bounds,
            items: vec![CaptureTarget {
                source: CaptureItemSource::Window(window.handle),
                bounds: source.bounds,
            }],
        })
    }

    fn all_screens_targets(&self) -> Result<CaptureTargets, ScreenshotCaptureError> {
        let context = self.context(None, None);
        let source = resolve_source(CaptureSourceMode::AllScreens, &context)
            .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
        let items = self
            .monitors
            .iter()
            .map(|monitor| CaptureTarget {
                source: CaptureItemSource::Monitor(monitor.handle),
                bounds: monitor.snapshot.bounds,
            })
            .collect();
        Ok(CaptureTargets {
            capture_bounds: source.bounds,
            output_bounds: source.bounds,
            items,
        })
    }

    fn region_targets(
        &self,
        region: PhysicalRect,
    ) -> Result<CaptureTargets, ScreenshotCaptureError> {
        let context = self.context(None, Some(region));
        let source = resolve_source(CaptureSourceMode::Region, &context)
            .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
        let capture_bounds = region_capture_bounds(&context.monitors, region)
            .map_err(|_| ScreenshotCaptureError::CaptureUnavailable)?;
        let items = self
            .monitors
            .iter()
            .filter(|monitor| monitor.snapshot.bounds.intersects(region))
            .map(|monitor| CaptureTarget {
                source: CaptureItemSource::Monitor(monitor.handle),
                bounds: monitor.snapshot.bounds,
            })
            .collect();
        Ok(CaptureTargets {
            capture_bounds,
            output_bounds: source.bounds,
            items,
        })
    }

    fn context(
        &self,
        active_window: Option<WindowSnapshot>,
        region: Option<PhysicalRect>,
    ) -> SourceResolveContext {
        SourceResolveContext {
            monitors: self
                .monitors
                .iter()
                .map(|monitor| monitor.snapshot.clone())
                .collect(),
            cursor: self.cursor,
            active_window,
            region,
        }
    }
}
