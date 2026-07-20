use super::super::timebase::{MediaTimebase, PausedInterval, TimebaseError};

#[test]
fn media_timebase_compacts_paused_intervals() -> Result<(), TimebaseError> {
    let timebase = MediaTimebase::new(1_000);
    let elapsed = timebase.elapsed_media_ms(
        11_000_000,
        &[PausedInterval {
            start_monotonic_ns: 2_000_000,
            end_monotonic_ns: 5_000_000,
        }],
    )?;

    assert_eq!(elapsed, 7);
    Ok(())
}

#[test]
fn media_timebase_rejects_invalid_times() {
    let timebase = MediaTimebase::new(1_000);

    assert!(matches!(
        timebase.elapsed_media_ms(999, &[]),
        Err(TimebaseError::BeforeStart)
    ));
    assert!(matches!(
        timebase.elapsed_media_ms(
            2_000,
            &[PausedInterval {
                start_monotonic_ns: 3_000,
                end_monotonic_ns: 2_000,
            }]
        ),
        Err(TimebaseError::InvalidPause)
    ));
}
