"""Typed controller failures and exit codes."""


class LabError(RuntimeError):
    """Expected, user-actionable lab failure."""

    exit_code = 1


class RunFailed(LabError):
    """A completed run produced a receipt but did not pass acceptance."""

    def __init__(self, run_id: str, message: str, mode: str) -> None:
        super().__init__(message)
        self.run_id = run_id
        self.mode = mode


class UsageError(LabError):
    """Invalid command-line request."""

    exit_code = 2


class IntegrityError(LabError):
    """Checksum, state, or guest-result integrity failure."""

    exit_code = 3


class TimeoutExpired(LabError):
    """A bounded lab operation exceeded its deadline."""

    exit_code = 4


class ResourceError(LabError):
    """Host resource policy rejected an operation."""

    exit_code = 5
