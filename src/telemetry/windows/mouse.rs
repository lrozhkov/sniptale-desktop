//! Windows mouse event normalization for recording telemetry.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowsMouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

impl From<WindowsMouseButton> for crate::protocol::trajectory::CursorTrajectoryButton {
    fn from(value: WindowsMouseButton) -> Self {
        use WindowsMouseButton::{Left, Middle, Right, X1, X2};
        match value {
            Left => Self::Left,
            Right => Self::Right,
            Middle => Self::Middle,
            X1 => Self::X1,
            X2 => Self::X2,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowsMouseEventKind {
    Move,
    Down(WindowsMouseButton),
    Up(WindowsMouseButton),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WindowsMouseEvent {
    pub(crate) sequence: u64,
    pub(crate) source_time_ms: u64,
    pub(crate) visible: Option<bool>,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) kind: WindowsMouseEventKind,
}

#[cfg(windows)]
#[derive(Debug)]
pub(crate) struct WindowsMouseSnapshot {
    pub(crate) events: Vec<WindowsMouseEvent>,
    pub(crate) dropped_events: u64,
    pub(crate) last_observation_seq: Option<u64>,
}
