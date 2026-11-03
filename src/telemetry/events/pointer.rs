//! Physical button actions retain proven pairs and last real movement without semantic guesses.

use super::emitter::EventEmitter;
use crate::telemetry::windows::WindowsMouseButton;
use crate::{
    protocol::recording_events::{
        EventAction, EventActionData, EventActionKind, EventActionUpdate, EventClickData,
        EventClickUpdate, EventCompletion, EventPoint, EventPreset, EventRecord,
    },
    telemetry::{
        CursorTrajectorySinkError, RecordingFrameGeometry,
        windows::{WindowsMouseEvent, WindowsMouseEventKind},
    },
};

#[derive(Debug)]
struct Gesture {
    id: String,
    down: u64,
    last: u64,
}

#[derive(Debug)]
pub(super) struct PointerHistory {
    frame: Option<RecordingFrameGeometry>,
    held: [Option<Gesture>; 32],
}

impl PointerHistory {
    pub(super) fn new(frame: Option<RecordingFrameGeometry>) -> Self {
        Self {
            frame,
            held: std::array::from_fn(|_| None),
        }
    }

    pub(super) fn observe(
        &mut self,
        event: WindowsMouseEvent,
        time: u64,
        output: &mut EventEmitter,
    ) -> Result<(), CursorTrajectorySinkError> {
        match event.kind {
            WindowsMouseEventKind::Move => {
                for gesture in self.held.iter_mut().flatten() {
                    gesture.last = time;
                }
            }
            WindowsMouseEventKind::Down(button) => {
                let button = button_number(button);
                let index = usize::from(button);
                if let Some(previous) = self.held[index].take() {
                    close(previous, None, None, time, output)?;
                }
                let id = output.id("click");
                let point = self.point(event.x, event.y);
                let action = EventAction {
                    id: id.clone(),
                    kind: EventActionKind::Click,
                    time_us: time,
                    duration_us: 0,
                    point,
                    label: "Click".to_owned(),
                    preset: EventPreset::None,
                    data: EventActionData::Click(EventClickData {
                        button,
                        gesture_id: Some(id.clone()),
                        gesture_type: None,
                        completion: EventCompletion::Observed,
                        down_time_us: Some(time),
                        up_time_us: None,
                        press_duration_us: None,
                        last_observed_time_us: time,
                        click_count: 1,
                        semantic_end_time_us: None,
                        drag_start_time_us: None,
                        pointer_cancelled: None,
                        drop_observed: None,
                        up_point: None,
                        ctrl_key: None,
                        alt_key: None,
                        shift_key: None,
                        meta_key: None,
                    }),
                };
                output.emit(|record_seq, segment_id| EventRecord::Action {
                    record_seq,
                    segment_id,
                    active_time_us: time,
                    action,
                })?;
                self.held[index] = Some(Gesture {
                    id,
                    down: time,
                    last: time,
                });
            }
            WindowsMouseEventKind::Up(button) => {
                if let Some(gesture) = self.held[usize::from(button_number(button))].take() {
                    close(
                        gesture,
                        Some(time),
                        self.point(event.x, event.y),
                        time,
                        output,
                    )?;
                }
            }
        }
        Ok(())
    }

    pub(super) fn boundary(
        &mut self,
        time: u64,
        output: &mut EventEmitter,
    ) -> Result<(), CursorTrajectorySinkError> {
        for gesture in &mut self.held {
            if let Some(gesture) = gesture.take() {
                close(gesture, None, None, time, output)?;
            }
        }
        Ok(())
    }

    fn point(&self, x: i32, y: i32) -> Option<EventPoint> {
        let frame = self.frame?;
        let (x, y) = frame.point(x, y)?;
        Some(EventPoint {
            x: f64::from(x) / f64::from(frame.width),
            y: f64::from(y) / f64::from(frame.height),
        })
    }
}

fn close(
    gesture: Gesture,
    up: Option<u64>,
    point: Option<EventPoint>,
    boundary: u64,
    output: &mut EventEmitter,
) -> Result<(), CursorTrajectorySinkError> {
    let data = EventActionUpdate::Click(EventClickUpdate {
        gesture_type: None,
        completion: if up.is_some() {
            EventCompletion::Released
        } else {
            EventCompletion::Lost
        },
        up_time_us: up,
        press_duration_us: up.and_then(|up| up.checked_sub(gesture.down)),
        last_observed_time_us: up.unwrap_or(gesture.last),
        semantic_end_time_us: None,
        drag_start_time_us: None,
        pointer_cancelled: None,
        drop_observed: None,
        up_point: point,
        ctrl_key: None,
        alt_key: None,
        shift_key: None,
        meta_key: None,
    });
    output.emit(|record_seq, segment_id| EventRecord::ActionUpdate {
        record_seq,
        segment_id,
        active_time_us: boundary,
        action_id: gesture.id,
        revision: 1,
        data,
    })
}

fn button_number(button: WindowsMouseButton) -> u8 {
    match button {
        WindowsMouseButton::Left => 0,
        WindowsMouseButton::Middle => 1,
        WindowsMouseButton::Right => 2,
        WindowsMouseButton::X1 => 3,
        WindowsMouseButton::X2 => 4,
    }
}
