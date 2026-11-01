use super::*;
use windows::Win32::Foundation::GetHandleInformation;

#[test]
fn initial_frame_deadline_cancels_silent_partial_header_and_partial_body()
-> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    use std::time::{Duration, Instant};
    for partial in [&[][..], &[8, 0][..], &[8, 0, 0, 0, 1][..]] {
        let (mut reader, mut writer) = io::pipe()?;
        writer.write_all(partial)?;
        let started = Instant::now();
        let result = crate::ipc::with_blocking_read_deadline(Duration::from_millis(100), || {
            crate::ipc::read_payload(&mut reader, 1024)
        });
        assert!(matches!(result, Err(error) if error.kind() == io::ErrorKind::TimedOut));
        assert!(started.elapsed() < Duration::from_secs(5));
        drop(writer);
    }
    Ok(())
}

#[test]
fn read_deadline_releases_watchdog_on_success_error_and_panic()
-> Result<(), Box<dyn std::error::Error>> {
    use std::time::Duration;
    assert!(with_blocking_read_deadline(Duration::ZERO, || ()).is_err());
    let frame = [2, 0, 0, 0, b'{', b'}'];
    let result = with_blocking_read_deadline(Duration::from_secs(1), || {
        crate::ipc::read_payload(&mut io::Cursor::new(frame), 1024)
    })??;
    assert_eq!(result, Some(b"{}".to_vec()));
    let failed = with_blocking_read_deadline(Duration::from_secs(1), || {
        crate::ipc::read_payload(&mut io::Cursor::new([1, 0]), 1024)
    })?;
    assert!(failed.is_err());
    let started = std::time::Instant::now();
    let panic = std::panic::catch_unwind(|| {
        let _ = with_blocking_read_deadline::<()>(Duration::from_secs(30), || {
            std::panic::resume_unwind(Box::new("synthetic read failure"));
        });
    });
    assert!(panic.is_err());
    assert!(started.elapsed() < Duration::from_secs(5));
    Ok(())
}

#[test]
fn cancellation_keeps_duplicated_handle_live_after_read_guard_drops()
-> Result<(), Box<dyn std::error::Error>> {
    let cancel = BlockingReadCancel::new();
    let guard = cancel.begin_read()?;
    let pending_cancel = cancel
        .state
        .thread
        .lock()
        .map_err(|_| "read handle lock")?
        .as_ref()
        .ok_or("missing duplicated handle")?
        .clone();
    let weak = Arc::downgrade(&pending_cancel);
    drop(guard);
    assert!(
        cancel
            .state
            .thread
            .lock()
            .map_err(|_| "read handle lock")?
            .is_none()
    );
    let mut flags = 0;
    // SAFETY: the cancellation owner retains the duplicated handle until this call completes.
    unsafe { GetHandleInformation(pending_cancel.0, &raw mut flags) }?;
    drop(pending_cancel);
    assert!(weak.upgrade().is_none());
    Ok(())
}

#[test]
fn cancellation_is_terminal_and_read_registration_releases_its_handle()
-> Result<(), Box<dyn std::error::Error>> {
    let cancel = BlockingReadCancel::new();
    let guard = cancel.begin_read()?;
    let other_thread = cancel.clone();
    std::thread::spawn(move || other_thread.cancel_pending_read())
        .join()
        .map_err(|_| "canceller panicked")?;
    assert!(cancel.is_cancelled());
    drop(guard);
    assert!(
        cancel
            .state
            .thread
            .lock()
            .map_err(|_| "read handle lock")?
            .is_none()
    );
    assert!(
        matches!(cancel.begin_read(), Err(error) if error.kind() == io::ErrorKind::ConnectionAborted)
    );
    Ok(())
}
