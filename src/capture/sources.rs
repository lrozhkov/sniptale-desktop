//! Monitor, window, virtual desktop, and region source resolution.

use super::coordinates::{CoordinateError, PhysicalPoint, PhysicalRect};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CaptureSourceMode {
    Screen,
    ActiveWindow,
    AllScreens,
    Region,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ResolvedSourceKind {
    Screen,
    ActiveWindow,
    AllScreens,
    Region,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SourceResolveError {
    Coordinate(CoordinateError),
    NoDisplays,
    CursorOutsideDisplays,
    ActiveWindowUnavailable,
    ActiveWindowMinimized,
    ActiveWindowCloaked,
    #[cfg(test)]
    ActiveWindowProtected,
    ActiveWindowUncapturable,
    RegionMissing,
    RegionOutsideVirtualDesktop,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MonitorSnapshot {
    pub(crate) id: String,
    pub(crate) bounds: PhysicalRect,
    pub(crate) is_primary: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WindowSnapshot {
    pub(crate) bounds: PhysicalRect,
    pub(crate) capture_state: WindowCaptureState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowCaptureState {
    Capturable,
    Minimized,
    Cloaked,
    #[cfg(test)]
    Protected,
    #[cfg(test)]
    Inaccessible,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SourceResolveContext {
    pub(crate) monitors: Vec<MonitorSnapshot>,
    pub(crate) cursor: Option<PhysicalPoint>,
    pub(crate) active_window: Option<WindowSnapshot>,
    pub(crate) region: Option<PhysicalRect>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedCaptureSource {
    pub(crate) kind: ResolvedSourceKind,
    pub(crate) bounds: PhysicalRect,
    pub(crate) monitor_id: Option<String>,
}

pub(crate) fn resolve_source(
    mode: CaptureSourceMode,
    context: &SourceResolveContext,
) -> Result<ResolvedCaptureSource, SourceResolveError> {
    let virtual_desktop = virtual_desktop_bounds(&context.monitors)?;
    match mode {
        CaptureSourceMode::Screen => resolve_screen(context),
        CaptureSourceMode::ActiveWindow => resolve_active_window(context, virtual_desktop),
        CaptureSourceMode::AllScreens => Ok(ResolvedCaptureSource {
            kind: ResolvedSourceKind::AllScreens,
            bounds: virtual_desktop,
            monitor_id: None,
        }),
        CaptureSourceMode::Region => resolve_region(context, virtual_desktop),
    }
}

pub(crate) fn virtual_desktop_bounds(
    monitors: &[MonitorSnapshot],
) -> Result<PhysicalRect, SourceResolveError> {
    let mut iter = monitors.iter();
    let first = iter.next().ok_or(SourceResolveError::NoDisplays)?.bounds;
    iter.try_fold(first, |bounds, monitor| {
        bounds
            .union(monitor.bounds)
            .map_err(SourceResolveError::Coordinate)
    })
}

pub(crate) fn region_capture_bounds(
    monitors: &[MonitorSnapshot],
    region: PhysicalRect,
) -> Result<PhysicalRect, SourceResolveError> {
    let mut intersecting = monitors
        .iter()
        .filter(|monitor| monitor.bounds.intersects(region));
    let first = intersecting
        .next()
        .ok_or(SourceResolveError::RegionOutsideVirtualDesktop)?
        .bounds;
    intersecting.try_fold(first, |bounds, monitor| {
        bounds
            .union(monitor.bounds)
            .map_err(SourceResolveError::Coordinate)
    })
}

fn resolve_screen(
    context: &SourceResolveContext,
) -> Result<ResolvedCaptureSource, SourceResolveError> {
    let monitor = match context.cursor {
        Some(cursor) => context
            .monitors
            .iter()
            .find(|monitor| monitor.bounds.contains(cursor))
            .ok_or(SourceResolveError::CursorOutsideDisplays)?,
        None => context
            .monitors
            .iter()
            .find(|monitor| monitor.is_primary)
            .or_else(|| context.monitors.first())
            .ok_or(SourceResolveError::NoDisplays)?,
    };
    Ok(ResolvedCaptureSource {
        kind: ResolvedSourceKind::Screen,
        bounds: monitor.bounds,
        monitor_id: Some(monitor.id.clone()),
    })
}

fn resolve_active_window(
    context: &SourceResolveContext,
    virtual_desktop: PhysicalRect,
) -> Result<ResolvedCaptureSource, SourceResolveError> {
    let window = context
        .active_window
        .as_ref()
        .ok_or(SourceResolveError::ActiveWindowUnavailable)?;
    match window.capture_state {
        WindowCaptureState::Capturable => {}
        WindowCaptureState::Minimized => return Err(SourceResolveError::ActiveWindowMinimized),
        WindowCaptureState::Cloaked => return Err(SourceResolveError::ActiveWindowCloaked),
        #[cfg(test)]
        WindowCaptureState::Protected => return Err(SourceResolveError::ActiveWindowProtected),
        #[cfg(test)]
        WindowCaptureState::Inaccessible => {
            return Err(SourceResolveError::ActiveWindowUncapturable);
        }
    }
    if !virtual_desktop.contains_rect(window.bounds) {
        return Err(SourceResolveError::ActiveWindowUncapturable);
    }
    Ok(ResolvedCaptureSource {
        kind: ResolvedSourceKind::ActiveWindow,
        bounds: window.bounds,
        monitor_id: None,
    })
}

fn resolve_region(
    context: &SourceResolveContext,
    virtual_desktop: PhysicalRect,
) -> Result<ResolvedCaptureSource, SourceResolveError> {
    let region = context.region.ok_or(SourceResolveError::RegionMissing)?;
    if !virtual_desktop.contains_rect(region) {
        return Err(SourceResolveError::RegionOutsideVirtualDesktop);
    }
    Ok(ResolvedCaptureSource {
        kind: ResolvedSourceKind::Region,
        bounds: region,
        monitor_id: None,
    })
}
