"""Exact alpha package acceptance using the existing isolated guest lifecycle."""
from __future__ import annotations

import json
import subprocess
import sys
from pathlib import Path

from errors import LabError
from results import parse_guest_result
from ssh import GuestSsh
from system import sha256_file
from windows_session import prepare_console

CASES = {"network-isolation", "clean-install", "first-run", "capture", "recording", "restart", "uninstall"}


def test_release(runner, lane_name: str, artifact: Path, run_dir: Path):
    bundle = artifact.resolve(strict=True).parent
    provenance_path = bundle / "provenance.json"
    if provenance_path.is_symlink() or provenance_path.stat().st_size > 4 * 1024 * 1024:
        raise LabError("oversized or redirected release provenance")
    try:
        provenance = json.loads(provenance_path.read_text(encoding="utf-8"))
    except (ValueError, UnicodeError) as exc:
        raise LabError("invalid release provenance JSON") from exc
    if not isinstance(provenance, dict) or not isinstance(provenance.get("tag"), str):
        raise LabError("invalid release provenance shape")
    commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=runner.paths.root, text=True).strip()
    verifier = runner.paths.root / "tooling/release/windows/alpha_artifacts.py"
    checked = subprocess.run([sys.executable, str(verifier), "verify", str(bundle), "--commit", commit,
                              "--tag", provenance.get("tag", "")], capture_output=True)
    if checked.returncode:
        raise LabError("release bundle identity or checksum verification failed")
    if artifact.name != f"SniptaleNativeAgent-dev-{provenance['productVersion']}.msi":
        raise LabError("MSI differs from release bundle declaration")
    output = run_dir / "guest" / lane_name
    output.mkdir(parents=True, exist_ok=True)
    runner.consumed_artifacts.extend(path for path in bundle.rglob("*") if path.is_file() and path != artifact)
    pre_forced = runner.vms.reset(lane_name)
    error = None
    try:
        runner.vms.start(lane_name, windows_audio_capture=True, host_reserve_mib=1024)
        runner._assert_windows_audio_backend(lane_name, require_capture=True)
        ssh = GuestSsh(runner.paths, runner.config.lanes[lane_name].ssh_port, "lab")
        ssh.wait(runner.config.timeouts.boot, probe=["cmd.exe", "/d", "/c", "exit", "0"])
        prepare_console(ssh, runner.config.timeouts.boot)
        ssh.upload(artifact, artifact.name, timeout=600)
        for name in ("release-run.ps1", "launch-release.ps1"):
            ssh.upload(runner.paths.root / "tooling/desktop/e2e_lab/guest/windows" / name, name, timeout=120)
        command = ["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", "launch-release.ps1",
                   "-Msi", artifact.name, "-MsiSha256", sha256_file(artifact),
                   "-ExpectedExeSha256", sha256_file(bundle / "sniptale-native-agent.exe"),
                   "-ExpectedManifestSha256", sha256_file(bundle / "native-host.json"),
                   "-ProductVersion", provenance["productVersion"], "-TimeoutSeconds", "1200"]
        try:
            ssh.command(command, timeout=1260, log=run_dir / "logs" / f"{lane_name}.log", stream=True)
        except LabError as exc:
            error = exc
        runner._assert_windows_audio_backend(lane_name, require_capture=True)
        runner._enforce_remote_evidence_limit(ssh, windows=True)
        ssh.download("e2e-out/", output, recursive=True, timeout=600,
                     max_bytes=runner._evidence_transfer_maximum(), max_files=2048)
    finally:
        runner._collect_qemu_log(lane_name, run_dir)
        runner._finish_vm(lane_name, "test", pre_forced)
    runner._validate_guest_tree(output)
    runner._sanitize_guest_text_tree(output)
    result_path = next(iter(output.rglob("result.json")), None)
    if result_path is None:
        raise error or LabError("release guest returned no result")
    result = parse_guest_result(result_path, expected_lane=lane_name, required_cases=CASES)
    if set(case.name for case in result.cases) != CASES:
        raise LabError("release guest case inventory mismatch")
    return result
