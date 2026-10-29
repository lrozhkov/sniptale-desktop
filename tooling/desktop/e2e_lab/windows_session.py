"""Prepare the synthetic console account on older clean Windows laboratory images."""
from __future__ import annotations

import time

from ssh import powershell_encoded


def prepare_console(ssh, timeout: int) -> None:
    # Key-based SSH can work while console autologon is blocked by password age.
    # This changes only the synthetic lab account, before any application is installed.
    ssh.command(powershell_encoded(
        "$ErrorActionPreference='Stop'; "
        "Set-LocalUser -Name lab -PasswordNeverExpires $true -FullName 'Sniptale Lab'; "
        "shutdown.exe /r /t 3"
    ), timeout=30)
    time.sleep(15)
    ssh.wait(timeout, probe=["cmd.exe", "/d", "/c", "exit", "0"])
