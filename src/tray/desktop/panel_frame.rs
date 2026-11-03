//! Bounded presentation geometry. Toolbar coordinates stay independent of overlay space.

use super::model::DesktopError;
use serde::{Deserialize, Serialize};

const MARGIN: f64 = 12.0;
const OVERLAY_SPACE: f64 = 320.0;

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PanelFrameRequest {
    pub(crate) first: bool,
    pub(crate) pixel_ratio: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) feedback_height: f64,
    pub(crate) overlay: bool,
    pub(crate) modal: bool,
    pub(crate) left: f64,
    pub(crate) top: f64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct FrameBounds {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PanelInsets {
    pub(crate) left: f64,
    pub(crate) top: f64,
    pub(crate) feedback_above: bool,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct PanelPlacement {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) insets: PanelInsets,
}

impl PanelFrameRequest {
    pub(crate) fn initial_origin(self, position: (f64, f64), size: (f64, f64)) -> (f64, f64) {
        let scale = self.pixel_ratio;
        if !self.first {
            return position;
        }
        (
            position.0 + (size.0 - self.width * scale) / 2.0 - self.left * scale,
            position.1 + (size.1 - self.height * scale) / 2.0 - self.top * scale,
        )
    }

    pub(crate) fn validate(self) -> Result<(), DesktopError> {
        let bounded =
            |value: f64, min: f64, max: f64| value.is_finite() && (min..=max).contains(&value);
        if !bounded(self.pixel_ratio, 0.5, 8.0)
            || !bounded(self.width, 1.0, 1024.0)
            || !bounded(self.height, 1.0, 256.0)
            || !bounded(self.feedback_height, 0.0, 240.0)
            || !bounded(self.left, 0.0, 768.0)
            || !bounded(self.top, 0.0, 768.0)
        {
            return Err(DesktopError::InvalidLayout);
        }
        Ok(())
    }

    pub(crate) fn place(
        self,
        origin: (f64, f64),
        work: FrameBounds,
    ) -> Result<PanelPlacement, DesktopError> {
        self.validate()?;
        let scale = self.pixel_ratio;
        if ![origin.0, origin.1, work.x, work.y]
            .iter()
            .all(|value| value.is_finite() && value.abs() <= 10_000_000.0)
            || !work.width.is_finite()
            || !work.height.is_finite()
            || !(64.0..=65536.0).contains(&work.width)
            || !(64.0..=65536.0).contains(&work.height)
        {
            return Err(DesktopError::InvalidLayout);
        }
        let available_width = work.width / scale - 2.0 * MARGIN;
        let available_height = work.height / scale - 2.0 * MARGIN;
        if available_width < 40.0 || available_height < 40.0 {
            return Err(DesktopError::InvalidLayout);
        }
        let bar_width = self.width.min(available_width);
        let bar_height = self.height.min(available_height);
        let anchor_x = (origin.0 + self.left * scale).clamp(
            work.x + MARGIN * scale,
            work.x + work.width - (bar_width + MARGIN) * scale,
        );
        let anchor_y = (origin.1 + self.top * scale).clamp(
            work.y + MARGIN * scale,
            work.y + work.height - (bar_height + MARGIN) * scale,
        );
        let above = ((anchor_y - work.y) / scale - MARGIN).max(0.0);
        let below = ((work.y + work.height - anchor_y) / scale - bar_height - MARGIN).max(0.0);
        let feedback = if self.feedback_height > 0.0 {
            self.feedback_height + MARGIN
        } else {
            0.0
        };
        let feedback_above = feedback > below && above > below;
        let mut reserve_above = if feedback_above {
            feedback.min(above)
        } else {
            0.0
        };
        let mut reserve_below = if feedback_above {
            0.0
        } else {
            feedback.min(below)
        };
        if self.overlay || self.modal {
            if below >= OVERLAY_SPACE || below >= above {
                reserve_below = reserve_below.max(OVERLAY_SPACE.min(below));
            } else {
                reserve_above = reserve_above.max(OVERLAY_SPACE.min(above));
            }
        }
        let content_width = if self.modal {
            bar_width.max(440.0_f64.min(available_width))
        } else {
            bar_width
        };
        let width = ((content_width + 2.0 * MARGIN) * scale)
            .ceil()
            .min(work.width);
        let height = ((bar_height + 2.0 * MARGIN + reserve_above + reserve_below) * scale)
            .ceil()
            .min(work.height);
        let x = (anchor_x - (MARGIN + (content_width - bar_width) / 2.0) * scale)
            .round()
            .clamp(work.x, work.x + work.width - width);
        let y = (anchor_y - (MARGIN + reserve_above) * scale)
            .round()
            .clamp(work.y, work.y + work.height - height);
        Ok(PanelPlacement {
            x,
            y,
            width,
            height,
            insets: PanelInsets {
                left: (anchor_x - x) / scale,
                top: (anchor_y - y) / scale,
                feedback_above,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request() -> PanelFrameRequest {
        PanelFrameRequest {
            first: false,
            pixel_ratio: 1.0,
            width: 500.0,
            height: 50.0,
            feedback_height: 0.0,
            overlay: false,
            modal: false,
            left: 12.0,
            top: 12.0,
        }
    }
    fn work() -> FrameBounds {
        FrameBounds {
            x: 0.0,
            y: 0.0,
            width: 1920.0,
            height: 1040.0,
        }
    }
    fn anchor(plan: PanelPlacement, scale: f64) -> (f64, f64) {
        (
            plan.x + plan.insets.left * scale,
            plan.y + plan.insets.top * scale,
        )
    }
    fn assert_number(actual: f64, expected: f64) {
        // This permits arithmetic roundoff only, not a screen-position tolerance.
        let roundoff = f64::EPSILON * expected.abs().max(1.0) * 8.0;
        assert!(
            (actual - expected).abs() <= roundoff,
            "{actual} != {expected}"
        );
    }
    fn assert_pair(actual: (f64, f64), expected: (f64, f64)) {
        assert_number(actual.0, expected.0);
        assert_number(actual.1, expected.1);
    }
    #[test]
    fn first_frame_centers_the_actual_toolbar_inside_the_initial_window() -> Result<(), DesktopError>
    {
        let mut request = request();
        request.first = true;
        let origin = request.initial_origin((100.0, 200.0), (760.0, 160.0));
        let plan = request.place(origin, work())?;
        let anchor = anchor(plan, 1.0);
        assert_pair(
            (
                anchor.0 + request.width / 2.0,
                anchor.1 + request.height / 2.0,
            ),
            (480.0, 280.0),
        );
        Ok(())
    }
    #[test]
    fn closed_frame_has_no_unused_popup_space() -> Result<(), DesktopError> {
        let plan = request().place((200.0, 300.0), work())?;
        assert_pair((plan.width, plan.height), (524.0, 74.0));
        assert_pair(anchor(plan, 1.0), (212.0, 312.0));
        Ok(())
    }
    #[test]
    fn overlays_and_feedback_preserve_anchor_at_both_vertical_edges() -> Result<(), DesktopError> {
        for y in [0.0, 970.0] {
            let mut request = request();
            let closed = request.place((100.0, y), work())?;
            request.overlay = true;
            request.feedback_height = 48.0;
            let open = request.place((100.0, y), work())?;
            assert_pair(anchor(open, 1.0), anchor(closed, 1.0));
            assert!(open.y >= 0.0 && open.y + open.height <= work().height);
            assert_eq!(open.insets.feedback_above, y > 0.0);
            request.overlay = false;
            request.feedback_height = 0.0;
            request.left = open.insets.left;
            request.top = open.insets.top;
            let restored = request.place((open.x, open.y), work())?;
            assert_pair(anchor(restored, 1.0), anchor(closed, 1.0));
            assert_number(restored.height, 74.0);
        }
        Ok(())
    }
    #[test]
    fn negative_monitors_scaling_and_modal_width_keep_content_inside_work_area()
    -> Result<(), DesktopError> {
        let work = FrameBounds {
            x: -1920.0,
            y: -120.0,
            width: 1920.0,
            height: 1080.0,
        };
        let mut request = request();
        request.width = 300.0;
        request.pixel_ratio = 1.5;
        let closed = request.place((-1800.0, 100.0), work)?;
        request.modal = true;
        let modal = request.place((-1800.0, 100.0), work)?;
        assert_pair(anchor(modal, 1.5), anchor(closed, 1.5));
        assert!(modal.x >= work.x && modal.x + modal.width <= work.x + work.width);
        assert!(modal.y >= work.y && modal.y + modal.height <= work.y + work.height);
        assert_number(modal.width, 696.0);
        Ok(())
    }
    #[test]
    fn invalid_geometry_is_rejected_before_window_operations() {
        for value in [f64::NAN, f64::INFINITY, -1.0, 2048.0] {
            let mut request = request();
            request.width = value;
            assert!(request.place((0.0, 0.0), work()).is_err());
        }
        assert!(
            PanelFrameRequest {
                pixel_ratio: 0.0,
                ..request()
            }
            .place((0.0, 0.0), work())
            .is_err()
        );
        assert!(request().place((f64::NAN, 0.0), work()).is_err());
    }
}
