//! Recording lifecycle state machine owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecordingState {
    Idle,
    Preparing,
    Recording,
    Paused,
    Stopping,
    Finalizing,
    Transferring,
    Completed,
    #[cfg(test)]
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecordingControl {
    Pause,
    Resume,
    Stop,
    Cancel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TerminalReason {
    Completed,
    Cancelled,
    #[cfg(test)]
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TransitionOutcome {
    Changed,
    IgnoredTerminal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecordingStateError {
    EmptyRecordingId,
    #[cfg(test)]
    TerminalReasonNotFailure,
    TransitionRejected {
        from: RecordingState,
        action: &'static str,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecordingSession {
    recording_id: Option<String>,
    state: RecordingState,
    terminal_reason: Option<TerminalReason>,
}

impl Default for RecordingSession {
    fn default() -> Self {
        Self {
            recording_id: None,
            state: RecordingState::Idle,
            terminal_reason: None,
        }
    }
}

impl RecordingSession {
    #[must_use]
    pub(crate) fn recording_id(&self) -> Option<&str> {
        self.recording_id.as_deref()
    }

    #[must_use]
    pub(crate) const fn state(&self) -> RecordingState {
        self.state
    }

    #[must_use]
    #[cfg(test)]
    pub(crate) const fn terminal_reason(&self) -> Option<TerminalReason> {
        self.terminal_reason
    }

    pub(crate) fn start(
        &mut self,
        recording_id: impl Into<String>,
    ) -> Result<TransitionOutcome, RecordingStateError> {
        self.ensure_not_terminal()?;
        if self.state != RecordingState::Idle {
            return Err(invalid(self.state, "start"));
        }
        let recording_id = recording_id.into();
        if recording_id.is_empty() {
            return Err(RecordingStateError::EmptyRecordingId);
        }
        self.recording_id = Some(recording_id);
        self.state = RecordingState::Preparing;
        Ok(TransitionOutcome::Changed)
    }

    pub(crate) fn begin_recording(&mut self) -> Result<TransitionOutcome, RecordingStateError> {
        match self.state {
            RecordingState::Preparing => {
                self.state = RecordingState::Recording;
                Ok(TransitionOutcome::Changed)
            }
            _ => self.invalid_or_terminal("recording"),
        }
    }

    pub(crate) fn apply_control(
        &mut self,
        control: RecordingControl,
    ) -> Result<TransitionOutcome, RecordingStateError> {
        match control {
            RecordingControl::Pause => {
                self.transition_from(RecordingState::Recording, RecordingState::Paused, "pause")
            }
            RecordingControl::Resume => {
                self.transition_from(RecordingState::Paused, RecordingState::Recording, "resume")
            }
            RecordingControl::Stop => match self.state {
                RecordingState::Recording | RecordingState::Paused => {
                    self.state = RecordingState::Stopping;
                    Ok(TransitionOutcome::Changed)
                }
                _ => self.invalid_or_terminal("stop"),
            },
            RecordingControl::Cancel => self.cancel(),
        }
    }

    pub(crate) fn mark_finalizing(&mut self) -> Result<TransitionOutcome, RecordingStateError> {
        self.transition_from(
            RecordingState::Stopping,
            RecordingState::Finalizing,
            "finalize",
        )
    }

    pub(crate) fn mark_transferring(&mut self) -> Result<TransitionOutcome, RecordingStateError> {
        self.transition_from(
            RecordingState::Finalizing,
            RecordingState::Transferring,
            "transfer",
        )
    }

    pub(crate) fn complete(&mut self) -> Result<TransitionOutcome, RecordingStateError> {
        if self.is_terminal() {
            return Ok(TransitionOutcome::IgnoredTerminal);
        }
        self.transition_terminal(
            RecordingState::Transferring,
            RecordingState::Completed,
            TerminalReason::Completed,
            "complete",
        )
    }

    #[cfg(test)]
    pub(crate) fn fail(
        &mut self,
        reason: TerminalReason,
    ) -> Result<TransitionOutcome, RecordingStateError> {
        if reason == TerminalReason::Completed {
            return Err(RecordingStateError::TerminalReasonNotFailure);
        }
        if self.is_terminal() {
            return Ok(TransitionOutcome::IgnoredTerminal);
        }
        self.state = RecordingState::Failed;
        self.terminal_reason = Some(reason);
        Ok(TransitionOutcome::Changed)
    }

    fn cancel(&mut self) -> Result<TransitionOutcome, RecordingStateError> {
        if self.is_terminal() {
            return Ok(TransitionOutcome::IgnoredTerminal);
        }
        match self.state {
            RecordingState::Preparing
            | RecordingState::Recording
            | RecordingState::Paused
            | RecordingState::Stopping => {
                self.state = RecordingState::Completed;
                self.terminal_reason = Some(TerminalReason::Cancelled);
                Ok(TransitionOutcome::Changed)
            }
            _ => self.invalid_or_terminal("cancel"),
        }
    }

    fn transition_from(
        &mut self,
        from: RecordingState,
        to: RecordingState,
        action: &'static str,
    ) -> Result<TransitionOutcome, RecordingStateError> {
        if self.is_terminal() {
            return Ok(TransitionOutcome::IgnoredTerminal);
        }
        if self.state != from {
            return Err(invalid(self.state, action));
        }
        self.state = to;
        Ok(TransitionOutcome::Changed)
    }

    fn transition_terminal(
        &mut self,
        from: RecordingState,
        to: RecordingState,
        reason: TerminalReason,
        action: &'static str,
    ) -> Result<TransitionOutcome, RecordingStateError> {
        self.transition_from(from, to, action)?;
        self.terminal_reason = Some(reason);
        Ok(TransitionOutcome::Changed)
    }

    fn invalid_or_terminal(
        &self,
        action: &'static str,
    ) -> Result<TransitionOutcome, RecordingStateError> {
        if self.is_terminal() {
            Ok(TransitionOutcome::IgnoredTerminal)
        } else {
            Err(invalid(self.state, action))
        }
    }

    fn ensure_not_terminal(&self) -> Result<(), RecordingStateError> {
        if self.is_terminal() {
            Err(invalid(self.state, "terminal"))
        } else {
            Ok(())
        }
    }

    const fn is_terminal(&self) -> bool {
        if matches!(self.state, RecordingState::Completed) {
            return true;
        }
        #[cfg(test)]
        if matches!(self.state, RecordingState::Failed) {
            return true;
        }
        false
    }
}

const fn invalid(from: RecordingState, action: &'static str) -> RecordingStateError {
    RecordingStateError::TransitionRejected { from, action }
}
