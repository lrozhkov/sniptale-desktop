//! Windows mouse event normalization for recording telemetry.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowsMouseButton {
    Left,
    Right,
    Middle,
}

impl WindowsMouseButton {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Middle => "middle",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowsMouseEventKind {
    Move,
    Click(WindowsMouseButton),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WindowsMouseEvent {
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) kind: WindowsMouseEventKind,
}
