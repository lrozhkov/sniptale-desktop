//! Native-host process log adapters.

use crate::app::bridge::NativeHostRuntimeLog;

pub(super) struct NativeHostProcessLog(pub(super) crate::diagnostics::ProcessLogSink);

impl NativeHostRuntimeLog for NativeHostProcessLog {
    #[cfg(not(test))]
    fn phase(&self, phase: &'static str, message: &str) {
        let _ = self.0.write_event_now(crate::diagnostics::ProcessLogEvent {
            level: crate::diagnostics::LogLevel::Info,
            target: "sniptale_native_agent::native_host_runtime",
            operation: "handshake",
            operation_id: None,
            phase,
            message,
            duration_ms: None,
            error: None,
        });
    }

    #[cfg(windows)]
    fn bridge_forward_log(&self) -> Option<crate::ipc::BridgeForwardLogSink> {
        Some(std::sync::Arc::new(NativeHostBridgeForwardLog(
            self.0.clone(),
        )))
    }
}

#[cfg(windows)]
struct NativeHostBridgeForwardLog(crate::diagnostics::ProcessLogSink);

#[cfg(windows)]
impl crate::ipc::BridgeForwardLog for NativeHostBridgeForwardLog {
    fn event(&self, phase: &'static str, message: &str, error_code: Option<&str>) {
        let error = error_code.map(|code| crate::diagnostics::LogErrorFields {
            code: code.to_owned(),
            recoverable: true,
        });
        let _ = self.0.write_event_now(crate::diagnostics::ProcessLogEvent {
            level: if error.is_some() {
                crate::diagnostics::LogLevel::Warn
            } else {
                crate::diagnostics::LogLevel::Info
            },
            target: "sniptale_native_agent::native_host_bridge",
            operation: "native-message-bridge",
            operation_id: None,
            phase,
            message,
            duration_ms: None,
            error: error.as_ref(),
        });
    }
}
