//! Freeze and region selection overlay owner.

#[cfg(windows)]
mod windows_region;

#[cfg(any(windows, test))]
use super::coordinates::{CoordinateError, PhysicalPoint, PhysicalRect, normalize_drag_selection};

#[cfg(windows)]
pub(crate) use windows_region::WindowsRegionSelector;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RegionOverlayAvailability {
    #[cfg_attr(
        not(any(windows, test)),
        expect(
            dead_code,
            reason = "Windows overlay adapter reports region-selection availability"
        )
    )]
    SelectionAvailable,
    Available,
    #[cfg_attr(
        windows,
        expect(
            dead_code,
            reason = "non-Windows adapter constructs unsupported-platform status"
        )
    )]
    UnsupportedPlatform,
    #[cfg_attr(
        not(any(windows, test)),
        expect(
            dead_code,
            reason = "capability contract keeps runtime-unavailable overlay status distinct"
        )
    )]
    #[cfg_attr(
        all(windows, not(test)),
        expect(
            dead_code,
            reason = "non-Windows/test capability facts construct runtime-unavailable overlay status"
        )
    )]
    RuntimeUnavailable,
}

pub(crate) fn runtime_region_overlay_availability() -> RegionOverlayAvailability {
    platform_region_overlay_availability()
}

#[cfg(windows)]
const fn platform_region_overlay_availability() -> RegionOverlayAvailability {
    RegionOverlayAvailability::SelectionAvailable
}

#[cfg(not(windows))]
const fn platform_region_overlay_availability() -> RegionOverlayAvailability {
    RegionOverlayAvailability::UnsupportedPlatform
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OverlayEvent {
    DragStart(PhysicalPoint),
    DragMove(PhysicalPoint),
    Confirm,
    #[cfg(windows)]
    Cancel,
    #[cfg(test)]
    DisplayChanged,
    #[cfg_attr(
        any(windows, test),
        expect(
            dead_code,
            reason = "shutdown event is reserved for overlay owner teardown"
        )
    )]
    Shutdown,
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OverlayCancelReason {
    #[cfg(windows)]
    UserCancelled,
    #[cfg(test)]
    DisplayChanged,
    Shutdown,
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OverlayOutcome {
    Pending,
    Confirmed(PhysicalRect),
    Cancelled(OverlayCancelReason),
    InvalidSelection,
}

#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RegionOverlayState {
    start: Option<PhysicalPoint>,
    current: Option<PhysicalPoint>,
    min_size_px: u32,
    released: bool,
}

#[cfg(any(windows, test))]
impl RegionOverlayState {
    pub(crate) const fn new(min_size_px: u32) -> Self {
        Self {
            start: None,
            current: None,
            min_size_px,
            released: false,
        }
    }

    pub(crate) fn apply(&mut self, event: OverlayEvent) -> OverlayOutcome {
        if self.released {
            return OverlayOutcome::Cancelled(OverlayCancelReason::Shutdown);
        }
        match event {
            OverlayEvent::DragStart(point) => self.start_drag(point),
            OverlayEvent::DragMove(point) => self.move_drag(point),
            OverlayEvent::Confirm => self.confirm(),
            #[cfg(windows)]
            OverlayEvent::Cancel => self.cancel(OverlayCancelReason::UserCancelled),
            #[cfg(test)]
            OverlayEvent::DisplayChanged => self.cancel(OverlayCancelReason::DisplayChanged),
            OverlayEvent::Shutdown => self.cancel(OverlayCancelReason::Shutdown),
        }
    }

    #[cfg(test)]
    pub(crate) const fn released(self) -> bool {
        self.released
    }

    pub(crate) fn preview_rect(self) -> Option<PhysicalRect> {
        let (Some(start), Some(current)) = (self.start, self.current) else {
            return None;
        };
        normalize_drag_selection(start, current, self.min_size_px).ok()
    }

    fn start_drag(&mut self, point: PhysicalPoint) -> OverlayOutcome {
        self.start = Some(point);
        self.current = Some(point);
        OverlayOutcome::Pending
    }

    fn move_drag(&mut self, point: PhysicalPoint) -> OverlayOutcome {
        if self.start.is_some() {
            self.current = Some(point);
        }
        OverlayOutcome::Pending
    }

    fn confirm(&mut self) -> OverlayOutcome {
        let (Some(start), Some(current)) = (self.start, self.current) else {
            return OverlayOutcome::InvalidSelection;
        };
        match normalize_drag_selection(start, current, self.min_size_px) {
            Ok(rect) => {
                self.released = true;
                OverlayOutcome::Confirmed(rect)
            }
            Err(CoordinateError::SelectionTooSmall | CoordinateError::EmptyRect) => {
                OverlayOutcome::InvalidSelection
            }
            Err(CoordinateError::Overflow) => OverlayOutcome::InvalidSelection,
        }
    }

    fn cancel(&mut self, reason: OverlayCancelReason) -> OverlayOutcome {
        self.released = true;
        OverlayOutcome::Cancelled(reason)
    }
}
