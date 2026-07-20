//! Startup/degraded-mode doctor check.

use crate::{
    app::resident::AgentStartupStatus,
    diagnostics::{DoctorCheck, DoctorCheckName, DoctorStatus, doctor_check},
};

pub(super) fn startup_doctor_check(status: AgentStartupStatus) -> DoctorCheck {
    doctor_check(
        DoctorCheckName::RuntimeStartup,
        startup_doctor_status(status),
        startup_doctor_detail(status),
    )
}

const fn startup_doctor_status(status: AgentStartupStatus) -> DoctorStatus {
    match status {
        AgentStartupStatus::Operational => DoctorStatus::Pass,
        AgentStartupStatus::RepairRequired
        | AgentStartupStatus::UnsupportedCacheSchema
        | AgentStartupStatus::RepairRequiredAndUnsupportedCacheSchema => DoctorStatus::Fail,
    }
}

const fn startup_doctor_detail(status: AgentStartupStatus) -> &'static str {
    match status {
        AgentStartupStatus::Operational => "runtime startup is operational",
        AgentStartupStatus::RepairRequired => {
            "runtime startup is degraded because install repair is required"
        }
        AgentStartupStatus::UnsupportedCacheSchema => {
            "runtime startup is degraded because native cache schema is unsupported"
        }
        AgentStartupStatus::RepairRequiredAndUnsupportedCacheSchema => {
            "runtime startup is degraded because install repair and native cache repair are required"
        }
    }
}
