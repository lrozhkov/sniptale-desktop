use super::super::{
    TelemetryHookEventQueue, TelemetryHookQueueSnapshot,
    hooks::{
        HookAvailability, HookAvailabilitySnapshot, HookFailure, HookPolicy, TelemetryHookKind,
        TelemetryHookPlan, plan_recording_hooks, release_order,
    },
    policy::RecordingTelemetryPolicy,
};

#[test]
fn hook_event_queue_is_bounded_and_reports_drops() {
    let mut queue = TelemetryHookEventQueue::new(2);

    queue.push("first");
    queue.push("second");
    queue.push("third");

    assert_eq!(
        queue.drain(),
        TelemetryHookQueueSnapshot {
            events: vec!["first", "second"],
            dropped_events: 1,
        }
    );
    assert_eq!(
        queue.drain(),
        TelemetryHookQueueSnapshot {
            events: Vec::new(),
            dropped_events: 0,
        }
    );
}

#[test]
fn hook_event_queue_zero_capacity_counts_all_drops() {
    let mut queue = TelemetryHookEventQueue::new(0);

    queue.push("first");
    queue.push("second");

    assert_eq!(
        queue.drain(),
        TelemetryHookQueueSnapshot {
            events: Vec::new(),
            dropped_events: 2,
        }
    );
}

#[test]
fn lifecycle_cutoff_drains_only_the_accepted_prefix_and_retains_future_facts() {
    let mut queue = TelemetryHookEventQueue::new(4);
    for time in [10, 20, 30, 40] {
        queue.push(time);
    }
    queue.push(50);
    let before_pause = queue.drain_while(|time| *time <= 20);
    assert_eq!(before_pause.events, [10, 20]);
    assert_eq!(before_pause.dropped_events, 1);
    assert_eq!(queue.front(), Some(&30));
    let paused = queue.drain_while(|time| *time <= 35);
    assert_eq!(paused.events, [30]);
    assert_eq!(paused.dropped_events, 0);
    assert_eq!(queue.front(), Some(&40));
    assert_eq!(
        queue.drain_while(|time| *time <= 35).events,
        Vec::<i32>::new()
    );
    assert_eq!(queue.drain().events, [40]);
}

#[test]
fn telemetry_hook_drop_counters_accumulate_saturated_burst_without_growth() {
    let mut queue = TelemetryHookEventQueue::new(3);

    for index in 0..32 {
        queue.push(index);
    }

    assert_eq!(
        queue.drain(),
        TelemetryHookQueueSnapshot {
            events: vec![0, 1, 2],
            dropped_events: 29,
        }
    );
    queue.push(33);
    assert_eq!(
        queue.drain(),
        TelemetryHookQueueSnapshot {
            events: vec![33],
            dropped_events: 0,
        }
    );
}

#[test]
fn hook_plan_installs_available_keyboard_and_mouse_hooks() -> Result<(), HookFailure> {
    let plan = plan_recording_hooks(
        HookAvailabilitySnapshot {
            keyboard: HookAvailability::Available,
            mouse: HookAvailability::Available,
        },
        HookPolicy {
            telemetry_non_fatal: false,
        },
    )?;

    assert_eq!(
        plan.install,
        vec![TelemetryHookKind::Keyboard, TelemetryHookKind::Mouse]
    );
    assert_eq!(plan.warnings.len(), 0);
    Ok(())
}

#[test]
fn default_telemetry_policy_requests_no_hooks() {
    assert_eq!(
        RecordingTelemetryPolicy::default()
            .requested_hook_kinds()
            .len(),
        0
    );
}

#[test]
fn hook_denial_is_warning_only_when_policy_allows_nonfatal_telemetry() {
    let plan = plan_recording_hooks(
        HookAvailabilitySnapshot {
            keyboard: HookAvailability::PermissionDenied,
            mouse: HookAvailability::Available,
        },
        HookPolicy {
            telemetry_non_fatal: true,
        },
    );

    assert_eq!(
        plan,
        Ok(TelemetryHookPlan {
            install: vec![TelemetryHookKind::Mouse],
            warnings: vec![HookFailure::PermissionDenied(TelemetryHookKind::Keyboard)],
        })
    );
}

#[test]
fn hook_denial_is_fatal_when_telemetry_is_required() {
    let plan = plan_recording_hooks(
        HookAvailabilitySnapshot {
            keyboard: HookAvailability::PolicyDenied,
            mouse: HookAvailability::Available,
        },
        HookPolicy {
            telemetry_non_fatal: false,
        },
    );

    assert_eq!(
        plan,
        Err(HookFailure::PolicyDenied(TelemetryHookKind::Keyboard))
    );
}

#[test]
fn hook_release_order_is_deterministic() {
    assert_eq!(
        release_order(&[TelemetryHookKind::Mouse, TelemetryHookKind::Keyboard]),
        vec![TelemetryHookKind::Keyboard, TelemetryHookKind::Mouse]
    );
}
