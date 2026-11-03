"""Verify desktop CDP drivers, owned frame handles, and guest source synchronization."""

from __future__ import annotations

import json
import subprocess
from pathlib import Path
import tempfile
import unittest


class UiAutomationLibraryTests(unittest.TestCase):
    def test_settings_close_requires_the_owned_native_frame_handle(self):
        root = Path(__file__).resolve().parents[4]
        version = json.loads((root / "tooling/configs/qa/tool-versions.json").read_text())["scriptTools"]["pwsh"][
            "version"
        ]
        script = r"""
param([string] $Repository)
$ErrorActionPreference = 'Stop'
. (Join-Path $Repository 'tooling/desktop/e2e_lab/guest/windows/windows-ui.ps1')
Add-Type @'
using System;
public static class SniptaleTrayProbe {
  public static int Posts;
  public static bool PostMessage(IntPtr window, uint message, UIntPtr parameter, IntPtr data) {
    if (window.ToInt64() != 17 || message != 0x0010) throw new Exception("wrong close target");
    Posts++;
    return true;
  }
  public static bool IsWindow(IntPtr window) { return false; }
}
'@
$script:frameHandle = 0
function Find-DesktopSettingsWindow([int] $Seconds) {
  [pscustomobject]@{Current=[pscustomobject]@{NativeWindowHandle=$script:frameHandle}}
}
try {
  Close-DesktopSettingsFrame
  throw 'zero frame handle was accepted'
} catch {
  if ($_.Exception.Message -ne 'settings frame has no native handle') { throw }
}
if ([SniptaleTrayProbe]::Posts -ne 0) { throw 'zero handle sent a thread message' }
$script:frameHandle = 17
Close-DesktopSettingsFrame
if ([SniptaleTrayProbe]::Posts -ne 1) { throw 'owned frame was not closed exactly once' }
"""
        with tempfile.TemporaryDirectory() as directory:
            probe = Path(directory) / "close-frame.ps1"
            probe.write_text(script)
            result = subprocess.run(
                [
                    str(root / f".tmp/qa-tools/script-tools/pwsh-{version}/pwsh"),
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-File",
                    str(probe),
                    str(root),
                ],
                capture_output=True,
                text=True,
                check=False,
                timeout=15,
            )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_desktop_cdp_driver_contracts(self):
        root = Path(__file__).resolve().parents[4]
        version = json.loads((root / "tooling/configs/qa/tool-versions.json").read_text())["browserAutomation"]["node"]
        for name, driver in (
            ("browser-playwright-contracts.cjs", "browser-playwright.cjs"),
            ("desktop-capture-contracts.cjs", "desktop-capture-playwright.cjs"),
        ):
            with self.subTest(driver=driver):
                result = subprocess.run(
                    [
                        str(root / f".tmp/qa-tools/node-{version}/bin/node"),
                        str(root / "tooling/desktop/e2e_lab/tests" / name),
                        str(root / "tooling/desktop/e2e_lab/guest/windows" / driver),
                    ],
                    capture_output=True,
                    text=True,
                    check=False,
                    timeout=30,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_product_wait_preserves_caller_condition_and_reports_failure(self):
        root = Path(__file__).resolve().parents[4]
        version = json.loads((root / "tooling/configs/qa/tool-versions.json").read_text())["scriptTools"]["pwsh"][
            "version"
        ]
        script = r"""
param([string] $Repository)
$ErrorActionPreference = 'Stop'
. (Join-Path $Repository 'tooling/desktop/e2e_lab/guest/windows/windows-ui.ps1')
$condition = 'caller-value'
Wait-ProductCondition -Condition { $condition -eq 'caller-value' } -Failure 'caller condition was shadowed' -Seconds 1
try {
  Wait-ProductCondition -Condition { $false } -Failure 'expected timeout' -Seconds 0
  throw 'missing timeout'
} catch { if ($_.Exception.Message -ne 'expected timeout') { throw } }
try {
  Wait-ProductCondition -Condition { throw 'expected predicate failure' } -Failure 'incorrect replacement'
  throw 'missing predicate failure'
} catch { if ($_.Exception.Message -ne 'expected predicate failure') { throw } }
"""
        with tempfile.TemporaryDirectory() as directory:
            probe = Path(directory) / "wait.ps1"
            probe.write_text(script)
            result = subprocess.run(
                [
                    str(root / f".tmp/qa-tools/script-tools/pwsh-{version}/pwsh"),
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-File",
                    str(probe),
                    str(root),
                ],
                capture_output=True,
                text=True,
                check=False,
                timeout=15,
            )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_source_cleanup_preserves_only_exact_generated_tauri_outputs(self):
        root = Path(__file__).resolve().parents[4]
        version = json.loads((root / "tooling/configs/qa/tool-versions.json").read_text())["scriptTools"]["pwsh"][
            "version"
        ]
        script = r"""
param([string] $Repository, [string] $Workspace)
$ErrorActionPreference = 'Stop'
$tokens = $null
$parseErrors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile(
  (Join-Path $Repository 'tooling/desktop/e2e_lab/guest/windows/build.ps1'), [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw 'build script parse failed' }
$function = $ast.Find({ param($node)
  $node -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $node.Name -eq 'Remove-ObsoleteGuestSource'
}, $true)
if (-not $function) { throw 'production cleanup function missing' }
. ([scriptblock]::Create($function.Extent.Text))
$kept = @('keep.rs', 'permissions/autogenerated/command.toml', 'gen/schemas/desktop-schema.json')
$removed = @('obsolete.rs', 'permissions/autogenerated-extra/stale.toml', 'gen/schemas-extra/stale.json')
foreach ($relative in @($kept + $removed)) {
  $path = Join-Path $Workspace $relative
  New-Item -ItemType Directory -Path ([IO.Path]::GetDirectoryName($path)) -Force | Out-Null
  [IO.File]::WriteAllText($path, 'synthetic source')
}
$generated = Join-Path $Workspace $kept[1]
$before = (Get-Item -LiteralPath $generated).LastWriteTimeUtc.Ticks
Remove-ObsoleteGuestSource -Workspace $Workspace -Incoming @{ 'keep.rs' = $true }
foreach ($relative in $kept) {
  if (-not (Test-Path -LiteralPath (Join-Path $Workspace $relative))) { throw "required output deleted: $relative" }
}
foreach ($relative in $removed) {
  if (Test-Path -LiteralPath (Join-Path $Workspace $relative)) { throw "stale source retained: $relative" }
}
if ((Get-Item -LiteralPath $generated).LastWriteTimeUtc.Ticks -ne $before) { throw 'generated input timestamp changed' }
"""
        with tempfile.TemporaryDirectory() as directory:
            probe = Path(directory) / "cleanup.ps1"
            probe.write_text(script)
            result = subprocess.run(
                [
                    str(root / f".tmp/qa-tools/script-tools/pwsh-{version}/pwsh"),
                    "-NoLogo",
                    "-NoProfile",
                    "-NonInteractive",
                    "-File",
                    str(probe),
                    str(root),
                    str(Path(directory) / "work"),
                ],
                capture_output=True,
                text=True,
                check=False,
                timeout=15,
            )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
