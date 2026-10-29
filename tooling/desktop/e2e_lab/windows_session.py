"""Prepare the synthetic console account on older clean Windows laboratory images."""
from __future__ import annotations

import re
import time

from errors import LabError, TimeoutExpired
from ssh import powershell_encoded


def prepare_console(ssh, timeout: int) -> None:
    # Key-based SSH can work while console autologon is blocked by password age.
    # Observe a new boot, rather than accepting the still-live pre-reboot SSH service.
    boot = ssh.command(powershell_encoded(
        "$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; "
        "(Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().Ticks"
    ), timeout=30).stdout.strip()
    if len(boot) > 19 or re.fullmatch(r"[0-9]{1,19}", boot) is None or int(boot) == 0:
        raise LabError("invalid Windows boot identity")
    # This changes only the synthetic lab account, before any application is installed.
    ssh.command(powershell_encoded(
        "$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; "
        "Set-LocalUser -Name lab -PasswordNeverExpires $true -FullName 'Sniptale Lab'; "
        "shutdown.exe /r /t 3"
    ), timeout=30)
    probe = powershell_encoded(
        "$ProgressPreference='SilentlyContinue'; $ErrorActionPreference='Stop'; "
        "$boot=(Get-CimInstance Win32_OperatingSystem).LastBootUpTime.ToUniversalTime().Ticks; "
        f"if ($boot -gt {boot} -and (Get-Process explorer -ErrorAction SilentlyContinue)) "
        "{ Write-Output 'console-ready' } else { Write-Output 'console-waiting' }"
    )
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            remaining = max(1, int(deadline - time.monotonic()))
            if ssh.command(probe, timeout=min(30, remaining)).stdout.strip() == "console-ready":
                return
        except LabError:
            # The transport and guest services can disappear while Windows restarts.
            pass
        time.sleep(min(2, max(0, deadline - time.monotonic())))
    raise TimeoutExpired("new Windows boot and interactive desktop did not become ready")
