//! Platform-neutral one-shot recovery policy for transient render-target loss.

pub(crate) fn recover_once<State, Error>(
    state: &mut State,
    mut attempt: impl FnMut(&State) -> Result<(), Error>,
    recreate: impl FnOnce() -> Result<State, Error>,
) -> Result<(), Error> {
    if attempt(state).is_ok() {
        return Ok(());
    }
    *state = recreate()?;
    attempt(state)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::recover_once;

    #[derive(Debug)]
    struct FakeRenderer {
        failures_remaining: Cell<u8>,
        attempts: Cell<u8>,
    }

    #[test]
    fn target_loss_recreates_once_and_retries_the_frame() {
        let mut renderer = FakeRenderer {
            failures_remaining: Cell::new(1),
            attempts: Cell::new(0),
        };
        let attempt = |renderer: &FakeRenderer| {
            renderer.attempts.set(renderer.attempts.get() + 1);
            if renderer.failures_remaining.get() > 0 {
                renderer
                    .failures_remaining
                    .set(renderer.failures_remaining.get() - 1);
                Err("device-lost")
            } else {
                Ok(())
            }
        };

        assert_eq!(
            recover_once(&mut renderer, attempt, || {
                Ok(FakeRenderer {
                    failures_remaining: Cell::new(0),
                    attempts: Cell::new(0),
                })
            }),
            Ok(())
        );
        assert_eq!(renderer.attempts.get(), 1);
    }

    #[test]
    fn persistent_failure_is_returned_after_one_recreation() {
        let mut renderer = FakeRenderer {
            failures_remaining: Cell::new(1),
            attempts: Cell::new(0),
        };
        assert_eq!(
            recover_once(
                &mut renderer,
                |_| Err("device-lost"),
                || Ok(FakeRenderer {
                    failures_remaining: Cell::new(1),
                    attempts: Cell::new(0),
                })
            ),
            Err("device-lost")
        );
    }
}
