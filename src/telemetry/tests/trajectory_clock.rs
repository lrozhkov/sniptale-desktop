use crate::telemetry::trajectory_clock::{
    TrajectoryActiveClock, TrajectoryClockError, WindowsTrajectoryClock,
    extend_windows_message_time,
};

#[test]
fn message_clock_wrap_and_equal_timestamps_preserve_source_time() -> Result<(), TrajectoryClockError>
{
    let wrap = 1_u64 << 32;
    assert_eq!(
        extend_windows_message_time(u32::MAX - 2, wrap + 5)?,
        wrap - 3
    );
    assert_eq!(extend_windows_message_time(4, wrap + 5)?, wrap + 4);
    assert!(extend_windows_message_time(0x8000_0000, 0).is_err());
    let clock = WindowsTrajectoryClock::calibrate(wrap + 5, 100_000_000, 100_002_000)?;
    let previous = clock.event_monotonic_ns(wrap - 3)?;
    let next = clock.event_monotonic_ns(wrap + 4)?;
    assert_eq!(next - previous, 7_000_000);
    assert_eq!(clock.event_monotonic_ns(wrap + 4)?, next);
    assert_eq!(clock.source_cutoff_ms(next)?, wrap + 4);
    assert_eq!(clock.source_cutoff_ms(next + 999_999)?, wrap + 4);
    assert_eq!(clock.source_cutoff_ms(next - 1)?, wrap + 3);
    assert_eq!(WindowsTrajectoryClock::metadata().resolution_bound_us, None);
    assert_eq!(
        WindowsTrajectoryClock::metadata().mapping_uncertainty_us,
        None
    );
    assert!(WindowsTrajectoryClock::calibrate(1, 2, 1).is_err());
    Ok(())
}

#[test]
fn pauses_are_removed_once_with_constant_state_and_late_events_are_not_rewritten()
-> Result<(), TrajectoryClockError> {
    let mut clock = TrajectoryActiveClock::new(1_000_000);
    assert_eq!(
        clock.observe(999_999),
        Err(TrajectoryClockError::BeforeStart)
    );
    assert_eq!(clock.observe(2_000_000)?, 1000);
    assert_eq!(clock.observe(2_000_000)?, 1000);
    assert_eq!(clock.pause(3_000_000)?, 2000);
    assert_eq!(clock.observe(3_000_001), Err(TrajectoryClockError::Paused));
    assert!(clock.resume(2_000_000).is_err());
    assert_eq!(clock.resume(8_000_000)?, 2000);
    assert_eq!(
        clock.observe(7_000_000),
        Err(TrajectoryClockError::LateObservation)
    );
    assert_eq!(clock.observe(9_000_000)?, 3000);
    for index in 0..10000_u64 {
        let pause = 10_000_000 + index * 2_000_000;
        clock.pause(pause)?;
        clock.resume(pause + 1_000_000)?;
    }
    assert_eq!(clock.stop(20_010_000_000)?, 10_004_000);
    Ok(())
}

#[test]
fn event_join_preserves_delayed_fact_times_but_never_reassigns_an_old_phase()
-> Result<(), TrajectoryClockError> {
    let mut clock = crate::telemetry::events_clock::EventActiveClock::new(0);
    assert_eq!(clock.observe(1_000_000)?, 1000);
    assert_eq!(clock.observe(3_000_000)?, 3000);
    assert_eq!(clock.observe(2_000_000)?, 2000);
    assert!(clock.pause(2_000_000).is_err());
    assert_eq!(clock.pause(4_000_000)?, 4000);
    assert_eq!(clock.stop(10_000_000)?, 4000);
    assert_eq!(clock.resume(8_000_000)?, 4000);
    assert_eq!(
        clock.observe(3_000_000),
        Err(TrajectoryClockError::LateObservation)
    );
    assert_eq!(clock.observe(9_000_000)?, 5000);
    assert_eq!(clock.stop(10_000_000)?, 6000);
    Ok(())
}
