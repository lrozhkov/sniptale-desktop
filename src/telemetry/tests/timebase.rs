use super::super::timebase::{MediaTimebase, TimebaseError};

#[test]
fn media_timebase_compacts_paused_intervals() -> Result<(), TimebaseError> {
    let mut timebase = MediaTimebase::new(1000);
    timebase.pause(2_000_000)?;
    timebase.resume(5_000_000)?;
    assert_eq!(timebase.elapsed_media_ms(11_000_000)?, 7);
    Ok(())
}

#[test]
fn media_timebase_rejects_invalid_times() -> Result<(), TimebaseError> {
    let mut timebase = MediaTimebase::new(1000);
    assert_eq!(
        timebase.elapsed_media_ms(999),
        Err(TimebaseError::BeforeStart)
    );
    timebase.pause(3000)?;
    assert_eq!(timebase.resume(2000), Err(TimebaseError::InvalidPause));
    timebase.resume(5000)?;
    assert_eq!(timebase.elapsed_media_ms(6000)?, 0);
    Ok(())
}
