"""Prepare the synthetic console account on older clean Windows laboratory images."""

from __future__ import annotations

import re
import time
from typing import Callable

from errors import LabError, TimeoutExpired
from ssh import powershell_encoded
from progress import emit


def prepare_console(ssh, timeout: int, *, health_check: Callable[[], None] | None = None) -> None:
    emit("console-readiness", status="checking")
    # Key-based SSH can work while console autologon is blocked by password age.
    # Observe a new boot, rather than accepting the still-live pre-reboot SSH service.
    boot = ssh.command(
        powershell_encoded(
            "$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; "
            "(Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().Ticks"
        ),
        timeout=min(timeout, 90),
    ).stdout.strip()
    if len(boot) > 19 or re.fullmatch(r"[0-9]{1,19}", boot) is None or int(boot) == 0:
        raise LabError("invalid Windows boot identity")
    ready = ssh.command(
        powershell_encoded(
            "$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; "
            "Set-LocalUser -Name lab -PasswordNeverExpires $true -FullName 'Sniptale Lab'; "
            "$console=(Get-CimInstance Win32_ComputerSystem).UserName; "
            "if ($console -match '\\\\lab$' -and (Get-Process explorer -ErrorAction SilentlyContinue)) "
            "{ Write-Output 'console-ready' } else { Write-Output 'console-waiting' }"
        ),
        timeout=min(timeout, 90),
    ).stdout.strip()
    if ready == "console-ready":
        if health_check is not None:
            health_check()
        emit("console-readiness", status="ready", rebooted=False)
        return
    # This changes only the synthetic lab account, before any application is installed.
    emit("console-readiness", status="recovering-account")
    ssh.command(
        powershell_encoded(
            "$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; "
            "Set-LocalUser -Name lab -PasswordNeverExpires $true -FullName 'Sniptale Lab'; "
            "shutdown.exe /r /t 3"
        ),
        timeout=30,
    )
    probe = powershell_encoded(
        "$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; "
        "$boot=(Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().Ticks; "
        f"if ($boot -gt {boot} -and (Get-Process explorer -ErrorAction SilentlyContinue)) "
        "{ Write-Output 'console-ready' } else { Write-Output 'console-waiting' }"
    )
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if health_check is not None:
            health_check()
        try:
            remaining = max(1, int(deadline - time.monotonic()))
            if ssh.command(probe, timeout=min(30, remaining)).stdout.strip() == "console-ready":
                emit("console-readiness", status="ready", rebooted=True)
                return
        except LabError:
            # The transport and guest services can disappear while Windows restarts.
            pass
        time.sleep(min(2, max(0, deadline - time.monotonic())))
    raise TimeoutExpired("new Windows boot and interactive desktop did not become ready")


def require_license(ssh, minimum_seconds: int) -> dict:
    """Reject expired or soon-expiring lab Windows without exposing product identifiers."""
    import json

    if type(minimum_seconds) is not int or not 1 <= minimum_seconds <= 86400:
        raise LabError("invalid Windows license time budget")
    output = (
        ssh.command(
            powershell_encoded(
                "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; "
                "$items=@(Get-CimInstance SoftwareLicensingProduct -Filter \"ApplicationID='55c92734-d682-4d71-983e-d6ec3f16059f' AND PartialProductKey IS NOT NULL\"); "
                "$rows=@($items | ForEach-Object {[ordered]@{status=[int]$_.LicenseStatus; "
                "grace_minutes=[int]$_.GracePeriodRemaining; evaluation=[bool]($_.Description -match 'TIMEBASED_EVAL')}}); "
                "[ordered]@{licenses=$rows} | ConvertTo-Json -Depth 4 -Compress"
            ),
            timeout=90,
        )
        .stdout.strip()
        .lstrip("\ufeff")
    )
    if len(output) > 16384:
        raise LabError("invalid Windows license response")
    try:
        value = json.loads(output)
    except (ValueError, TypeError) as exc:
        raise LabError("invalid Windows license response") from exc
    if (
        not isinstance(value, dict)
        or set(value) != {"licenses"}
        or not isinstance(value["licenses"], list)
        or not 1 <= len(value["licenses"]) <= 16
    ):
        raise LabError("invalid Windows license response")
    for row in value["licenses"]:
        if (
            not isinstance(row, dict)
            or set(row) != {"status", "grace_minutes", "evaluation"}
            or type(row["status"]) is not int
            or not 0 <= row["status"] <= 6
            or type(row["grace_minutes"]) is not int
            or not 0 <= row["grace_minutes"] <= 5256000
            or type(row["evaluation"]) is not bool
        ):
            raise LabError("invalid Windows license response")
    required_minutes = (minimum_seconds + 59) // 60 + 5
    for row in value["licenses"]:
        permanent = row["status"] == 1 and not row["evaluation"] and row["grace_minutes"] == 0
        timed = row["status"] in (1, 2, 3) and row["grace_minutes"] >= required_minutes
        if permanent or timed:
            emit("windows-license", status="ready", evaluation=row["evaluation"], graceMinutes=row["grace_minutes"])
            return row
    raise LabError(
        "Windows license expired or has insufficient time for this operation; refresh the isolated golden image before continuing"
    )
