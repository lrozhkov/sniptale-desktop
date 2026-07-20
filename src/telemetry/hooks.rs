//! Temporary keyboard and mouse hook owner.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TelemetryHookKind {
    Keyboard,
    Mouse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HookAvailability {
    Available,
    #[cfg(test)]
    PermissionDenied,
    #[cfg(test)]
    PolicyDenied,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HookFailure {
    #[cfg(test)]
    PermissionDenied(TelemetryHookKind),
    #[cfg(test)]
    PolicyDenied(TelemetryHookKind),
    #[cfg(windows)]
    Blocked(TelemetryHookKind),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HookPolicy {
    pub(crate) telemetry_non_fatal: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HookAvailabilitySnapshot {
    pub(crate) keyboard: HookAvailability,
    pub(crate) mouse: HookAvailability,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TelemetryHookPlan {
    pub(crate) install: Vec<TelemetryHookKind>,
    pub(crate) warnings: Vec<HookFailure>,
}

#[cfg(test)]
pub(crate) fn plan_recording_hooks(
    availability: HookAvailabilitySnapshot,
    policy: HookPolicy,
) -> Result<TelemetryHookPlan, HookFailure> {
    plan_requested_recording_hooks(
        &[TelemetryHookKind::Keyboard, TelemetryHookKind::Mouse],
        availability,
        policy,
    )
}

pub(crate) fn plan_requested_recording_hooks(
    requested_hooks: &[TelemetryHookKind],
    availability: HookAvailabilitySnapshot,
    policy: HookPolicy,
) -> Result<TelemetryHookPlan, HookFailure> {
    let mut plan = TelemetryHookPlan {
        install: Vec::new(),
        warnings: Vec::new(),
    };
    if requested_hooks.contains(&TelemetryHookKind::Keyboard) {
        add_hook(
            &mut plan,
            TelemetryHookKind::Keyboard,
            availability.keyboard,
            policy,
        )?;
    }
    if requested_hooks.contains(&TelemetryHookKind::Mouse) {
        add_hook(
            &mut plan,
            TelemetryHookKind::Mouse,
            availability.mouse,
            policy,
        )?;
    }
    Ok(plan)
}

#[cfg(test)]
pub(crate) fn release_order(active_hooks: &[TelemetryHookKind]) -> Vec<TelemetryHookKind> {
    let mut release = Vec::new();
    if active_hooks.contains(&TelemetryHookKind::Keyboard) {
        release.push(TelemetryHookKind::Keyboard);
    }
    if active_hooks.contains(&TelemetryHookKind::Mouse) {
        release.push(TelemetryHookKind::Mouse);
    }
    release
}

fn add_hook(
    plan: &mut TelemetryHookPlan,
    kind: TelemetryHookKind,
    availability: HookAvailability,
    policy: HookPolicy,
) -> Result<(), HookFailure> {
    #[cfg(not(test))]
    let _ = policy;
    match availability {
        HookAvailability::Available => {
            plan.install.push(kind);
            Ok(())
        }
        #[cfg(test)]
        HookAvailability::PermissionDenied => {
            denied(plan, policy, HookFailure::PermissionDenied(kind))
        }
        #[cfg(test)]
        HookAvailability::PolicyDenied => denied(plan, policy, HookFailure::PolicyDenied(kind)),
    }
}

#[cfg(test)]
fn denied(
    plan: &mut TelemetryHookPlan,
    policy: HookPolicy,
    failure: HookFailure,
) -> Result<(), HookFailure> {
    if policy.telemetry_non_fatal {
        plan.warnings.push(failure);
        Ok(())
    } else {
        Err(failure)
    }
}
