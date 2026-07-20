//! Windows Graphics Capture session owner.

#[cfg(any(windows, test))]
mod recording_owner;
#[cfg(windows)]
mod windows;

#[cfg(test)]
use super::{coordinates::CoordinateError, sources::ResolvedCaptureSource};

#[cfg(windows)]
pub(crate) use self::windows::WindowsWgcFrameProvider;
#[cfg(windows)]
pub(crate) use self::windows::recording_source_bounds;
#[cfg(windows)]
pub(crate) use self::windows::{WgcRecordingFrameError, WindowsWgcRecordingSession};

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WgcPlanError {
    InvalidSource(CoordinateError),
    ProtectedContent,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WgcSourcePolicy {
    pub(crate) protected_content: bool,
}

#[cfg(test)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct WgcFramePlan {
    pub(crate) source: ResolvedCaptureSource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WgcRuntimeAvailability {
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
            reason = "Windows Graphics Capture adapter reports API-unavailable on Windows"
        )
    )]
    ApiUnavailable,
}

#[cfg(test)]
pub(crate) fn plan_frame_capture(
    source: ResolvedCaptureSource,
    policy: WgcSourcePolicy,
) -> Result<WgcFramePlan, WgcPlanError> {
    if policy.protected_content {
        return Err(WgcPlanError::ProtectedContent);
    }
    super::coordinates::PhysicalRect::new(
        source.bounds.x,
        source.bounds.y,
        source.bounds.width,
        source.bounds.height,
    )
    .map_err(WgcPlanError::InvalidSource)?;
    Ok(WgcFramePlan { source })
}

pub(crate) fn runtime_capture_availability() -> WgcRuntimeAvailability {
    platform_capture_availability()
}

#[cfg(windows)]
fn platform_capture_availability() -> WgcRuntimeAvailability {
    match ::windows::Graphics::Capture::GraphicsCaptureSession::IsSupported() {
        Ok(true) => WgcRuntimeAvailability::Available,
        Ok(false) | Err(_) => WgcRuntimeAvailability::ApiUnavailable,
    }
}

#[cfg(not(windows))]
const fn platform_capture_availability() -> WgcRuntimeAvailability {
    WgcRuntimeAvailability::UnsupportedPlatform
}
