//! Typed UI requests share the resident tray queue and owner projections.

use super::super::WorkerCommand;
use super::projection::CaptureProjection;
use crate::{
    settings::ActiveSettings,
    tray::desktop::model::{
        Availability, CaptureCapabilities, CaptureQuality, CaptureRequest, CaptureSnapshot,
        CaptureTargets, DesktopBackend, DesktopError, Permission, PreferencesPatch, UiPreferences,
    },
};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc::{self, SyncSender, TrySendError},
};

pub(super) struct ResidentDesktopBackend {
    settings: ActiveSettings,
    projection: CaptureProjection,
    tray: SyncSender<WorkerCommand>,
    operational: bool,
    window_available: AtomicBool,
    frame: Mutex<Option<CaptureSnapshot>>,
    admission: Mutex<()>,
    preferences_path: Option<std::path::PathBuf>,
    preference_updates: Mutex<Option<UiPreferences>>,
}

impl ResidentDesktopBackend {
    pub(super) fn new(
        settings: ActiveSettings,
        projection: CaptureProjection,
        tray: SyncSender<WorkerCommand>,
        operational: bool,
    ) -> Self {
        Self {
            settings,
            projection,
            tray,
            operational,
            window_available: AtomicBool::new(false),
            frame: Mutex::new(None),
            admission: Mutex::new(()),
            preferences_path: crate::app::platform::ui_preferences_path(),
            preference_updates: Mutex::new(None),
        }
    }
    pub(super) fn observe_window_available(&self, available: bool) {
        self.window_available.store(available, Ordering::Release);
    }

    pub(super) fn flush_preferences_update(
        &self,
        publish: impl FnOnce(UiPreferences) -> Result<(), DesktopError>,
    ) -> Result<(), DesktopError> {
        let mut pending = self
            .preference_updates
            .lock()
            .map_err(|_| DesktopError::Unavailable)?;
        if let Some(value) = *pending {
            publish(value)?;
            *pending = None;
        }
        Ok(())
    }
}

impl DesktopBackend for ResidentDesktopBackend {
    fn snapshot(&self) -> Result<CaptureSnapshot, DesktopError> {
        // Serialize the entire read so a late request cannot regress metadata at an equal revision.
        let mut frame = self.frame.lock().map_err(|_| DesktopError::Unavailable)?;
        let profile = self
            .settings
            .profile_snapshot()
            .map_err(|_| DesktopError::Unavailable)?;
        let quality = profile
            .manual_recording_settings()
            .map(|settings| settings.requested_quality.quality);
        let quality = match quality {
            Some(crate::protocol::recording::NativeVideoQuality::Low) => CaptureQuality::Low,
            Some(crate::protocol::recording::NativeVideoQuality::Medium) => {
                CaptureQuality::Balanced
            }
            Some(crate::protocol::recording::NativeVideoQuality::Ultra) => CaptureQuality::Ultra,
            _ => CaptureQuality::High,
        };
        let setup_required = profile.manual_destination().is_none();
        let available = Availability {
            supported: cfg!(windows),
            permission: Permission::NotRequired,
            source_available: self.operational && !setup_required,
        };
        let capabilities = CaptureCapabilities {
            screenshot: available,
            video: available,
            targets: CaptureTargets {
                region: available,
                display: available,
                window: Availability {
                    source_available: available.source_available
                        && self.window_available.load(Ordering::Acquire),
                    ..available
                },
            },
        };
        let mut next = self.projection.snapshot(capabilities, quality)?;
        next.setup_required = setup_required;
        if !available.supported || !available.source_available {
            next.commands
                .retain(|command| *command != crate::tray::desktop::model::CaptureCommand::Start);
        }
        if let Some(previous) = frame.as_ref() {
            next.revision = previous.revision;
            if next != *previous {
                next.revision = previous
                    .revision
                    .saturating_add(1)
                    .min(9_007_199_254_740_991);
            }
        }
        *frame = Some(next.clone());
        Ok(next)
    }

    fn command(&self, request: CaptureRequest) -> Result<(), DesktopError> {
        // One UI operation may wait for selection; reject excess requests before queueing.
        let _admission = self.admission.try_lock().map_err(|_| DesktopError::Busy)?;
        if !self.operational {
            return Err(DesktopError::Unavailable);
        }
        let (result_sender, result_receiver) = mpsc::sync_channel(1);
        self.tray
            .try_send(WorkerCommand::DesktopCapture {
                request,
                result_sender,
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => DesktopError::Busy,
                TrySendError::Disconnected(_) => DesktopError::Unavailable,
            })?;
        result_receiver
            .recv()
            .map_err(|_| DesktopError::Unavailable)?
    }

    fn settings(
        &self,
        request: crate::tray::desktop::settings::SettingsRequest,
    ) -> Result<crate::tray::desktop::settings::SettingsSnapshot, DesktopError> {
        let _admission = self.admission.try_lock().map_err(|_| DesktopError::Busy)?;
        let (result_sender, receiver) = mpsc::sync_channel(1);
        self.tray
            .try_send(WorkerCommand::DesktopSettings {
                request,
                result_sender,
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => DesktopError::Busy,
                TrySendError::Disconnected(_) => DesktopError::Unavailable,
            })?;
        receiver.recv().map_err(|_| DesktopError::Unavailable)?
    }

    fn preferences(&self) -> Result<UiPreferences, DesktopError> {
        crate::settings::load_ui_preferences_from_path(self.preferences_path.clone())
            .map_err(|_| DesktopError::Unavailable)
    }

    fn save_preferences(&self, patch: PreferencesPatch) -> Result<(), DesktopError> {
        if patch.theme.is_none() && patch.locale.is_none() {
            return Err(DesktopError::InvalidPreferences);
        }
        // Keep commit and pending-event order together. A failed event remains retryable.
        let mut pending = self
            .preference_updates
            .lock()
            .map_err(|_| DesktopError::Unavailable)?;
        let saved = crate::settings::update_ui_preferences_from_path(
            self.preferences_path.clone(),
            patch.theme,
            patch.locale,
        )
        .map_err(|_| DesktopError::Unavailable)?;
        *pending = Some(saved);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
