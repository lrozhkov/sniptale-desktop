//! Shared, bounded connection state for the tray projection.

use std::sync::{Arc, Mutex};

use crate::tray::TrayConnectionStatus;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TrayConnectionProjection {
    status: TrayConnectionStatus,
    generation: Option<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct TrayConnectionState(Arc<Mutex<TrayConnectionProjection>>);

impl Default for TrayConnectionState {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(TrayConnectionProjection {
            status: TrayConnectionStatus::Disconnected,
            generation: None,
        })))
    }
}

impl TrayConnectionState {
    pub(crate) fn status(&self) -> TrayConnectionStatus {
        self.0
            .lock()
            .map_or(TrayConnectionStatus::Disconnected, |projection| {
                projection.status
            })
    }

    pub(crate) fn set_connected(&self, generation: u64) {
        if let Ok(mut projection) = self.0.lock() {
            *projection = TrayConnectionProjection {
                status: TrayConnectionStatus::Connected,
                generation: Some(generation),
            };
        }
    }

    pub(crate) fn disconnect_if_current(&self, generation: u64) -> bool {
        let Ok(mut projection) = self.0.lock() else {
            return false;
        };
        if projection.generation != Some(generation) {
            return false;
        }
        *projection = TrayConnectionProjection {
            status: TrayConnectionStatus::Disconnected,
            generation: None,
        };
        true
    }

    #[cfg(any(windows, test))]
    pub(crate) fn clear(&self) {
        if let Ok(mut projection) = self.0.lock() {
            *projection = TrayConnectionProjection {
                status: TrayConnectionStatus::Disconnected,
                generation: None,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TrayConnectionState;
    use crate::tray::TrayConnectionStatus;

    #[test]
    fn connection_state_projects_cross_worker_disconnect() {
        let state = TrayConnectionState::default();
        let observer = state.clone();

        state.set_connected(1);
        assert_eq!(observer.status(), TrayConnectionStatus::Connected);
        assert!(state.disconnect_if_current(1));
        assert_eq!(observer.status(), TrayConnectionStatus::Disconnected);
    }

    #[test]
    fn stale_disconnect_does_not_clear_newer_controller_projection() {
        let state = TrayConnectionState::default();

        state.set_connected(1);
        state.set_connected(2);

        assert!(!state.disconnect_if_current(1));
        assert_eq!(state.status(), TrayConnectionStatus::Connected);
        assert!(state.disconnect_if_current(2));
    }
}
