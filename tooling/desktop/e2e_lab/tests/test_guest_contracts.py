from pathlib import Path
import unittest


GUEST = Path(__file__).resolve().parents[1] / "guest"


class GuestContractTests(unittest.TestCase):
    def test_linux_golden_disables_cloud_init_network_regeneration(self) -> None:
        script = (GUEST / "linux-provision.sh").read_text(encoding="utf-8")
        self.assertIn("99-sniptale-disable-network-config.cfg", script)
        self.assertIn("network: {config: disabled}", script)
        self.assertIn("renderer: networkd", script)
        self.assertIn("52:54:00:12:34:56", script)
        self.assertIn("10.0.2.15/24", script)

    def test_linux_browser_sources_are_https_and_fingerprint_bound(self) -> None:
        script = (GUEST / "linux-provision.sh").read_text(encoding="utf-8")
        self.assertIn("https://dl.google.com/linux/chrome/deb/", script)
        self.assertIn("EB4C1BFD4F042F6DDDCCEC917721F63BD38B4796", script)
        self.assertIn("35BAA0B33E9EB396F59CA838C0BA5CE6DC6315A3", script)
        self.assertNotIn("chromium-browser firefox", script)

    def test_windows_runner_proves_network_pipe_msi_and_both_audio_paths(self) -> None:
        script = (GUEST / "windows/run.ps1").read_text(encoding="utf-8")
        self.assertTrue(script.isascii())
        self.assertNotIn("'@ |", script)
        self.assertNotIn("attempt-$attempt:", script)
        for required in (
            'Add-Test "network-isolation"', "pipe-auth-v2-hmac-sha256",
            'Add-Test "recording-system-audio"',
            'Add-Test "recording-microphone"', "PreviousExeSha256",
            "Remove-Item -LiteralPath $installed -Force",
            '-replace "(?i)https?://[^\\s`\"\']+"',
            "[IO.Path]::IsPathRooted($Output)",
            "Provision-DevRuntimeTrust",
            '"attempt-${attempt}:start-not-observed"',
            '"missing-audio-track"',
            "$recordingReady = $false",
            "Ctrl+Alt+Shift+F5",
            'FindWindow("Shell_TrayWnd", $null)',
            '"trusted-connections.json"',
            "foreach ($listenerAttempt in 1..3)",
            "foreach ($browserAttempt in 1..3)",
            "New-SyntheticToneWave",
            "Test-NonSilentPcm",
            "function Reset-RecordingFixtureState",
            'if ($SourceMode -eq "microphone") {\n    Reset-RecordingFixtureState $media $tempMedia',
            '$profile.manualProfile.video.quality = "low"',
            '$profile.manualProfile.video.fps = "fps24"',
            'recording_fixture_quality = "low-fps24"',
            'microphone_fixture = "qemu-hda-alsa-infinite-synthetic-noise"',
            "function Initialize-SniptaleHotkeyInput",
            "function Initialize-SniptaleTrayProbe",
            "Initialize-SniptaleHotkeyInput",
            "PlaySync()",
            "960000",
            "192000",
            '"audio-too-short-${pcmBytes}-bytes"',
            '"audio-silent"',
            "GetWindowRect(window",
            "regionAttemptOutcomes",
            'system_audio_fixture = "qemu-hda-wav-duplex-880hz-s16le-48000-stereo"',
            "IdentityVerifierSha256",
            '-ExpectedProductVersion "0.0.1"',
            '-ExpectedProductVersion "0.1.0"',
            'Where-Object { $_.Extension -ne ".jsonl" }',
            "$folderY = $dialogBounds.Bottom - 75",
            '$resultDeadline = [DateTime]::UtcNow.AddSeconds(60)',
            "foreach ($protocolAttempt in 1..4)",
            '$existingRecordings = @{}',
            '$acceptedRecording = $candidate',
            "$minimumPcmBytes",
            "same rejected file until the deadline cannot change its contents",
            'sniptale-e2e-result-$ProbeNonce-$Name-$ExpectedStatus',
            "Invoke-VideoTrayAction",
            'Find-Ui "Sniptale E2E Probe Running" 15',
            "Write-ProgressEvent",
            '$selectedTests.Contains($Name)',
            'Write-ProgressEvent $scenario "retry" $attempt 2 55',
            'Write-ProgressEvent $scenario "finalizing" $attempt 2 20',
            'Write-ProgressEvent $scenario "stop-delivery" $stopAttempt 3 3',
            '$attemptFailure = "runtime-discarded"',
            '"attempt-${attempt}:stop-hotkey-not-observed"',
            '$attemptFailure = "finalization-stalled-${writingBytes}-bytes"',
            'function Get-AgentHotkeyEventCount',
            'function Get-AgentRecordingFailure',
            'function Write-AgentRecordingDiagnostics',
        ):
            self.assertIn(required, script)
        self.assertNotIn("PlayLooping()", script)
        self.assertNotIn("extension-managed resident exited during tray render", script)
        self.assertNotIn("Speech.Synthesis", script)
        self.assertNotIn("mftrace.exe", script)
        self.assertEqual(script.count("public static class SniptaleHotkeyInput"), 1)
        self.assertEqual(script.count("public static class SniptaleTrayProbe"), 1)
        self.assertLess(
            script.index("function Initialize-SniptaleHotkeyInput"),
            script.index('Add-Test "recording-system-audio"'),
        )
        recording_helper = script[
            script.index("function Invoke-SyntheticRecording"):
            script.index('Add-Test "recording-system-audio"')
        ]
        self.assertNotIn("foreach ($attempt in 1..3)", recording_helper)
        self.assertIn("Initialize-SniptaleHotkeyInput", recording_helper)
        probe = (GUEST / "browser-extension/probe.js").read_text(encoding="utf-8")
        self.assertIn("}, 45000);", probe)
        self.assertIn("sniptale-e2e-result-${nonce}-${browser}-${status}", probe)
        self.assertIn("stale-controller-lease", probe)
        self.assertIn("staleCommandSent && code === 'stale-controller-lease'", probe)
        self.assertNotIn("stale-owner command sent", probe)
        launcher = (GUEST / "windows/launch-interactive.ps1").read_text(encoding="utf-8")
        self.assertIn("interactive-entry.json", launcher)
        self.assertIn("@parameters", launcher)
        self.assertIn("interactive-task.log", launcher)
        self.assertIn("interactive-entry.cmd", launcher)
        self.assertIn('New-ScheduledTaskAction -Execute "C:\\Windows\\System32\\cmd.exe"', launcher)
        self.assertIn("-RunLevel Limited", launcher)
        self.assertNotIn("-RunLevel Highest", launcher)
        self.assertIn('[guest-progress] $_', launcher)
        self.assertIn('logs\\progress.log', launcher)
        self.assertIn("interactive scheduled task disappeared before result after bounded restart", launcher)
        self.assertIn('`"scenario`":`"interactive-launch`"', launcher)
        self.assertIn("$taskStartAttempts -lt 2", launcher)

    def test_windows_provisioning_is_restartable_and_stage_marked(self) -> None:
        script = (GUEST / "windows/provision.ps1").read_text(encoding="utf-8")
        self.assertIn('Get-NetFirewallRule -Name "SniptaleLabSsh"', script)
        self.assertIn("winget.exe list --source winget --id", script)
        self.assertIn('@("install", "--source", "winget"', script)
        self.assertIn("$attempt -le 3", script)
        self.assertIn('@("1.1.1.1", "8.8.8.8")', script)
        self.assertIn('-ServerAddresses "10.0.2.3"', script)
        self.assertIn("powercfg /hibernate off", script)
        self.assertIn('Set-ProvisionStage "openssh"', script)
        self.assertIn('Set-ProvisionStage "complete"', script)

    def test_windows_build_keeps_release_symbols_for_bounded_lab_diagnostics(self) -> None:
        script = (GUEST / "windows/build.ps1").read_text(encoding="utf-8")
        self.assertIn('$env:CARGO_PROFILE_RELEASE_DEBUG = "1"', script)
        self.assertIn('sniptale-native-agent-$Profile.pdb', script)

    def test_windows_build_requires_packaged_resources(self) -> None:
        script = (GUEST / "windows/build.ps1").read_text(encoding="utf-8")
        self.assertIn('$env:SNIPTALE_WINDOWS_RESOURCES = "require"', script)
        self.assertIn('$env:SNIPTALE_DEV_LOCAL_BROWSER_TRUST = "1"', script)
        self.assertIn('$env:SNIPTALE_INSTALL_CHANNEL = "dev"', script)
        self.assertNotIn('$env:SNIPTALE_CHANNEL =', script)
        self.assertIn('-d "ProjectLicense=', script)
        self.assertIn('-d "ProjectNotice=', script)
        self.assertIn('-d "ThirdPartyNotices=', script)
        self.assertIn('-d "ThirdPartyDependencies=', script)
        self.assertIn('& $identityVerifier -Msi $msi -ExpectedProductVersion $productVersion', script)
        self.assertIn('foreach ($productVersion in @("0.0.1", "0.1.0"))', script)

    def test_windows_installer_preserves_authorization_state_during_major_upgrade(self) -> None:
        root = GUEST.parents[3]
        wix = (root / "installer/wix/SniptaleNativeAgent.wxs").read_text(encoding="utf-8")
        builder = (GUEST / "windows/build.ps1").read_text(encoding="utf-8")
        runner = (GUEST / "windows/run.ps1").read_text(encoding="utf-8")
        self.assertIn('Condition="REMOVE=&quot;ALL&quot; AND NOT UPGRADINGPRODUCTCODE"', wix)
        self.assertIn('Id="RemoveAuthorizationStateOnTrueUninstall"', wix)
        self.assertNotIn('<RemoveFile Id="RemovePipeSecret"', wix)
        self.assertIn("Write-UpgradeAuthorizationFixture", runner)
        self.assertIn("major upgrade removed authorization state", runner)
        self.assertIn("major upgrade changed authorization state", runner)
        self.assertIn('Id="ThirdPartyNoticesMarkdownFile" Guid="{AD1936C4-9944-43AC-A6F0-17726ED05B19}"', wix)
        self.assertNotIn("<RemoveFile ", wix)
        self.assertIn('& $wix build $currentWixPath', builder)
        self.assertIn('-d "ThirdPartyNotices=$(Join-Path $work \'THIRD_PARTY_NOTICES.md\')"', builder)
        self.assertIn('& $IdentityVerifier -Msi $PreviousMsi -ExpectedProductVersion "0.0.1"', runner)
        self.assertIn('& $IdentityVerifier -Msi $Msi -ExpectedProductVersion "0.1.0"', runner)
        self.assertIn('SniptaleNativeAgent-dev-$productVersion-$Profile.msi', builder)


if __name__ == "__main__":
    unittest.main()
