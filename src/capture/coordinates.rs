//! DPI and virtual desktop coordinate normalization.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CoordinateError {
    EmptyRect,
    SelectionTooSmall,
    Overflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PhysicalPoint {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PhysicalRect {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl PhysicalRect {
    pub(crate) const fn new(
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    ) -> Result<Self, CoordinateError> {
        if width == 0 || height == 0 {
            Err(CoordinateError::EmptyRect)
        } else {
            Ok(Self {
                x,
                y,
                width,
                height,
            })
        }
    }

    pub(crate) fn right_exclusive(self) -> Result<i32, CoordinateError> {
        let width = i32::try_from(self.width).map_err(|_| CoordinateError::Overflow)?;
        self.x.checked_add(width).ok_or(CoordinateError::Overflow)
    }

    pub(crate) fn bottom_exclusive(self) -> Result<i32, CoordinateError> {
        let height = i32::try_from(self.height).map_err(|_| CoordinateError::Overflow)?;
        self.y.checked_add(height).ok_or(CoordinateError::Overflow)
    }

    pub(crate) fn contains(self, point: PhysicalPoint) -> bool {
        let Ok(right) = self.right_exclusive() else {
            return false;
        };
        let Ok(bottom) = self.bottom_exclusive() else {
            return false;
        };
        point.x >= self.x && point.x < right && point.y >= self.y && point.y < bottom
    }

    pub(crate) fn contains_rect(self, other: Self) -> bool {
        let Ok(right) = other.right_exclusive() else {
            return false;
        };
        let Ok(bottom) = other.bottom_exclusive() else {
            return false;
        };
        self.contains(PhysicalPoint {
            x: other.x,
            y: other.y,
        }) && self.contains(PhysicalPoint {
            x: right - 1,
            y: bottom - 1,
        })
    }

    pub(crate) fn intersects(self, other: Self) -> bool {
        let (Ok(self_right), Ok(self_bottom), Ok(other_right), Ok(other_bottom)) = (
            self.right_exclusive(),
            self.bottom_exclusive(),
            other.right_exclusive(),
            other.bottom_exclusive(),
        ) else {
            return false;
        };
        self.x < other_right
            && other.x < self_right
            && self.y < other_bottom
            && other.y < self_bottom
    }

    pub(crate) fn union(self, other: Self) -> Result<Self, CoordinateError> {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        let right = self.right_exclusive()?.max(other.right_exclusive()?);
        let bottom = self.bottom_exclusive()?.max(other.bottom_exclusive()?);
        let width = u32::try_from(right - left).map_err(|_| CoordinateError::Overflow)?;
        let height = u32::try_from(bottom - top).map_err(|_| CoordinateError::Overflow)?;
        Self::new(left, top, width, height)
    }
}

pub(crate) fn normalize_drag_selection(
    start: PhysicalPoint,
    end: PhysicalPoint,
    min_size_px: u32,
) -> Result<PhysicalRect, CoordinateError> {
    let x = start.x.min(end.x);
    let y = start.y.min(end.y);
    let width = start.x.abs_diff(end.x);
    let height = start.y.abs_diff(end.y);
    if width < min_size_px || height < min_size_px {
        return Err(CoordinateError::SelectionTooSmall);
    }
    PhysicalRect::new(x, y, width, height)
}

#[cfg(test)]
pub(crate) fn source_local_point(
    point: PhysicalPoint,
    source: PhysicalRect,
) -> Option<PhysicalPoint> {
    if !source.contains(point) {
        return None;
    }
    Some(PhysicalPoint {
        x: point.x - source.x,
        y: point.y - source.y,
    })
}
