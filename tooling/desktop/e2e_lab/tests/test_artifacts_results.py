from __future__ import annotations

import json
import base64
import hashlib
import io
import os
import subprocess
import sys
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from artifacts import create_source_bundle, evidence_manifest
from build_cache import restore as restore_build_cache, store as store_build_cache
from browser_probe import build as build_browser_probe
from errors import IntegrityError, LabError
from evidence import _required_evidence_directories, verify_run_evidence
import main as lab_main
from linux_artifacts import inspect_linux_artifact
from probe_package import build as build_probe_package
from provision import Provisioner
from paths import LabPaths
from receipt import redact, redact_data, validate_receipt
from results import GuestResult, TestCase, parse_guest_result, write_junit
from runner import LaneRunner, expand_windows_cases
from system import sha256_file


class ArtifactResultTests(unittest.TestCase):
    def test_failed_rerun_resolves_supplied_msi_by_receipt_hash_from_persistent_cache(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            paths = LabPaths.discover(Path(directory) / "repo")
            paths.ensure()
            previous = paths.runs / "20260717T120000Z"
            previous.mkdir()
            cached = paths.cache / "builds/windows-11/release/fingerprint/out/SniptaleNativeAgent-dev-0.1.0-release.msi"
            cached.parent.mkdir(parents=True)
            cached.write_bytes(b"receipt-bound-msi")
            receipt = {
                "artifacts": [{
                    "name": cached.name,
                    "sha256": sha256_file(cached),
                    "bytes": cached.stat().st_size,
                }],
            }

            self.assertEqual(
                lab_main._resolve_rerun_windows_msi(paths, previous, receipt),
                cached.resolve(),
            )

    def test_focused_audio_requires_recording_but_not_unrelated_screenshot_evidence(self) -> None:
        expected = {("windows-11", "recording-system-audio", "pass", 1)}
        receipt = {
            "run_mode": "focused-test",
            "result": "pass",
            "requested_cases": ["recording-system-audio"],
        }

        self.assertEqual(
            _required_evidence_directories(receipt, expected),
            {"logs", "recordings"},
        )

    def test_focused_capture_requires_screenshot_but_not_recording_evidence(self) -> None:
        expected = {("windows-11", "capture", "pass", 1)}
        receipt = {
            "run_mode": "focused-test",
            "result": "pass",
            "requested_cases": ["capture"],
        }

        self.assertEqual(
            _required_evidence_directories(receipt, expected),
            {"logs", "screenshots"},
        )

    def test_windows_case_expansion_keeps_focused_rerun_prerequisites(self) -> None:
        expanded = expand_windows_cases({"recording-system-audio"})
        self.assertEqual(
            expanded,
            {
                "network-isolation", "install", "upgrade", "repair", "resident",
                "recording-system-audio", "uninstall",
            },
        )
        self.assertEqual(
            expand_windows_cases({"network-isolation"}),
            {"network-isolation", "install", "upgrade", "repair", "resident", "uninstall"},
        )
        self.assertEqual(
            expand_windows_cases({"capture"}),
            {
                "network-isolation", "install", "upgrade", "repair", "resident", "tray",
                "capture", "uninstall",
            },
        )
        with self.assertRaisesRegex(LabError, "unknown Windows E2E"):
            expand_windows_cases({"unknown-case"})

    def test_rerun_failed_finds_nested_cached_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            paths = LabPaths.discover(root)
            paths.ensure()
            run_id = "20260717T120000Z"
            previous = paths.lab / run_id
            artifacts = previous / "artifacts/windows-11/out"
            artifacts.mkdir(parents=True)
            current = artifacts / "SniptaleNativeAgent-dev-0.1.0-release.msi"
            current.write_bytes(b"msi")
            (previous / "receipt.json").write_text(json.dumps({
                "artifacts": [{
                    "name": current.name,
                    "sha256": sha256_file(current),
                    "bytes": current.stat().st_size,
                }],
                "test_matrix": [{
                    "lane": "windows-11",
                    "tests": [{"name": "recording-system-audio", "status": "fail"}],
                }],
            }), encoding="utf-8")
            with (
                mock.patch.object(lab_main, "verify_run_evidence"),
                mock.patch.object(lab_main, "execute_run", return_value="focused") as execute,
            ):
                result = lab_main.failed_windows_rerun(paths, SimpleNamespace(), run_id)
            self.assertEqual(result, "focused")
            self.assertEqual(execute.call_args.args[4], current)
            self.assertEqual(execute.call_args.args[5], {"recording-system-audio"})

    def test_windows_build_cache_is_fingerprint_bound_and_tamper_evident(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            root.mkdir()
            paths = LabPaths.discover(root)
            paths.ensure()
            source = root / "build-output"
            source.mkdir()
            for name, content in {
                "SniptaleNativeAgent-dev-0.1.0-release.msi": b"current-msi",
                "SniptaleNativeAgent-dev-0.0.1-release.msi": b"baseline-msi",
                "sniptale-native-agent-release.exe": b"current-exe",
                "sniptale-native-agent-baseline-release.exe": b"baseline-exe",
                "native-host.json": b"{}",
            }.items():
                (source / name).write_bytes(content)
            fingerprint = "a" * 64
            store_build_cache(paths, "windows-11", "release", fingerprint, source)
            restored = root / "restored"
            artifacts = restore_build_cache(
                paths, "windows-11", "release", fingerprint, restored
            )
            self.assertIsNotNone(artifacts)
            self.assertEqual((restored / "native-host.json").read_bytes(), b"{}")
            self.assertIsNone(
                restore_build_cache(
                    paths, "windows-11", "release", "b" * 64, root / "miss"
                )
            )
            cache_file = (
                paths.cache / "builds/windows-11/release" / fingerprint /
                "sniptale-native-agent-release.exe"
            )
            cache_file.write_bytes(b"tampered")
            self.assertIsNone(
                restore_build_cache(
                    paths, "windows-11", "release", fingerprint, root / "tampered"
                )
            )
    def test_execute_run_records_every_requested_lane_after_first_failure(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            paths = LabPaths.discover(root)
            paths.ensure()
            artifact = root / "artifact.msi"
            artifact.write_bytes(b"msi")
            source = SimpleNamespace(
                archive=paths.lab / "unused-source.tar.gz",
                sha256="a" * 64,
                file_count=1,
                commit_sha="b" * 40,
                dirty_sha256="c" * 64,
            )

            class FailingRunner:
                def __init__(self, _paths, _config):
                    self.teardowns = {}
                    self.consumed_artifacts = []

                def test(self, lane, _artifact, _run_dir):
                    raise LabError(f"{lane} failed")

            lanes = ["windows-11", "linux-wayland", "linux-x11"]
            snapshots = {lane: {"disk_sha256": str(index) * 64} for index, lane in enumerate(lanes, 1)}
            with (
                mock.patch.object(lab_main, "create_source_bundle", return_value=source),
                mock.patch.object(lab_main, "LaneRunner", FailingRunner),
                mock.patch.object(lab_main, "snapshot_identities", return_value=snapshots),
                mock.patch.object(lab_main, "gc"),
                self.assertRaisesRegex(LabError, "windows-11 failed"),
            ):
                lab_main.execute_run(paths, SimpleNamespace(), lanes, "release", artifact)
            run_dir = next(path for path in paths.runs.iterdir() if path.name[:8].isdigit())
            receipt = json.loads((run_dir / "receipt.json").read_text(encoding="utf-8"))
            self.assertEqual([item["lane"] for item in receipt["test_matrix"]], lanes)
            self.assertTrue(all(item["tests"][0]["status"] == "fail" for item in receipt["test_matrix"]))
            self.assertEqual(receipt["durations_ms"]["linux-wayland"], 0)
            self.assertEqual(receipt["durations_ms"]["linux-x11"], 0)

    def test_downloaded_guest_text_is_redacted_before_receipt_processing(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            guest = Path(directory)
            (guest / "early.log").write_text(
                "Authorization: Bearer super-secret-token C:\\Users\\lab\\private.txt",
                encoding="utf-8",
            )
            (guest / "agent.jsonl").write_text(json.dumps({
                "operation": "recording-start",
                "recordingId": "recording-secret-1",
                "token": "super-secret-token",
            }) + "\n", encoding="utf-8")
            (guest / "result.json").write_text(json.dumps({
                "schema_version": 1,
                "lane": "windows-11",
                "tests": [
                    {"name": "recording-system-audio", "status": "pass", "duration_ms": 1, "detail": "token=super-secret-token"},
                    {"name": "recording-microphone", "status": "pass", "duration_ms": 2, "detail": "ok"},
                ],
                "environment": {"token": "super-secret-token", "display": "win32"},
                "artifacts": ["capture.png"],
            }), encoding="utf-8")
            binary = b"\x00super-secret-token\xff"
            (guest / "capture.png").write_bytes(binary)
            LaneRunner._sanitize_guest_text_tree(guest)
            self.assertNotIn("super-secret-token", (guest / "early.log").read_text(encoding="utf-8"))
            self.assertNotIn("private.txt", (guest / "early.log").read_text(encoding="utf-8"))
            agent = json.loads((guest / "agent.jsonl").read_text(encoding="utf-8"))
            self.assertEqual(agent["recordingId"], "[redacted]")
            self.assertEqual(agent["token"], "[redacted]")
            result = json.loads((guest / "result.json").read_text(encoding="utf-8"))
            self.assertEqual([case["name"] for case in result["tests"]], ["recording-system-audio", "recording-microphone"])
            self.assertEqual(result["environment"]["token"], "[redacted]")
            self.assertNotIn("super-secret-token", json.dumps(result))
            self.assertEqual((guest / "capture.png").read_bytes(), binary)

    def test_guest_evidence_rejects_unknown_or_oversized_text_before_reading(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            guest = Path(directory)
            unknown = guest / "transcript.yaml"
            unknown.write_text("token: super-secret-token", encoding="utf-8")
            later_log = guest / "z-later.log"
            later_log.write_text("token=super-secret-token", encoding="utf-8")
            with self.assertRaisesRegex(LabError, "unsupported file type"):
                LaneRunner._sanitize_guest_text_tree(guest)
            self.assertFalse(unknown.exists())
            self.assertNotIn("super-secret-token", later_log.read_text(encoding="utf-8"))

        with tempfile.TemporaryDirectory() as directory:
            guest = Path(directory)
            oversized = guest / "oversized.log"
            oversized.write_bytes(b"x" * (4 * 1024 * 1024 + 1))
            with self.assertRaisesRegex(LabError, "per-file limit"):
                LaneRunner._sanitize_guest_text_tree(guest)
            self.assertFalse(oversized.exists())

    def test_collect_verifier_rejects_junit_receipt_drift(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run = Path(directory) / "20260713T120000Z"
            for name in ("logs", "screenshots", "recordings"):
                (run / name).mkdir(parents=True, exist_ok=True)
                (run / name / f"{name}.txt").write_text("synthetic", encoding="utf-8")
            manifest = {"schema_version": 1, "artifacts": []}
            manifest_path = run / "artifact-manifest.json"
            manifest_path.write_text(json.dumps(manifest, sort_keys=True) + "\n", encoding="utf-8")
            evidence_manifest_path = run / "evidence-manifest.json"
            evidence_manifest_path.write_text(
                json.dumps(evidence_manifest(run), sort_keys=True) + "\n", encoding="utf-8"
            )
            snapshot = {"linux-wayland": {"disk_sha256": "d" * 64}}
            (run / "environment.json").write_text(json.dumps({
                "schema_version": 1, "lanes": snapshot, "host": {}, "guests": {},
            }), encoding="utf-8")
            case = TestCase("session", "pass", 1, "ok")
            write_junit(run / "junit.xml", [GuestResult("linux-wayland", (case,), {}, ())])
            receipt = {
                "schema_version": 1, "run_id": run.name,
                "source": {"sha256": "a" * 64, "file_count": 1},
                "commit_sha": "b" * 40, "dirty_state_sha256": "c" * 64,
                "artifact_manifest_sha256": sha256_file(manifest_path),
                "evidence_manifest_sha256": sha256_file(evidence_manifest_path), "artifacts": [],
                "lanes": ["linux-wayland"], "snapshots": snapshot,
                "test_matrix": [{"lane": "linux-wayland", "tests": [{
                    "name": "session", "status": "pass", "duration_ms": 1, "detail": "ok",
                }]}],
                "durations_ms": {"linux-wayland": 1, "total": 1},
                "teardown": {"linux-wayland": {"test_pre_forced": False, "test_post_forced": False}},
                "result": "pass",
            }
            (run / "receipt.json").write_text(json.dumps(receipt), encoding="utf-8")
            self.assertTrue(verify_run_evidence(run)["verified"])
            log_file = run / "logs/logs.txt"
            log_file.write_text("tampered", encoding="utf-8")
            with self.assertRaisesRegex(IntegrityError, "receipt-bound manifest"):
                verify_run_evidence(run)
            log_file.write_text("synthetic", encoding="utf-8")
            receipt["test_matrix"][0]["tests"].append({
                "name": "session", "status": "fail", "duration_ms": 2, "detail": "duplicate",
            })
            (run / "receipt.json").write_text(json.dumps(receipt), encoding="utf-8")
            with self.assertRaisesRegex(IntegrityError, "duplicate case identities"):
                verify_run_evidence(run)
            receipt["test_matrix"][0]["tests"].pop()
            receipt["test_matrix"][0]["tests"][0]["duration_ms"] = 2
            (run / "receipt.json").write_text(json.dumps(receipt), encoding="utf-8")
            with self.assertRaisesRegex(IntegrityError, "JUnit results differ"):
                verify_run_evidence(run)
            receipt["test_matrix"][0]["tests"][0]["duration_ms"] = 1
            (run / "receipt.json").write_text(json.dumps(receipt), encoding="utf-8")
            junit = (run / "junit.xml").read_text(encoding="utf-8").replace('name="session"', 'name="different"')
            (run / "junit.xml").write_text(junit, encoding="utf-8")
            with self.assertRaisesRegex(IntegrityError, "JUnit results differ"):
                verify_run_evidence(run)
            write_junit(run / "junit.xml", [GuestResult("linux-wayland", (case,), {}, ())])
            receipt["lanes"].append("linux-x11")
            receipt["snapshots"]["linux-x11"] = {"disk_sha256": "e" * 64}
            receipt["teardown"]["linux-x11"] = {"test_pre_forced": False, "test_post_forced": False}
            (run / "receipt.json").write_text(json.dumps(receipt), encoding="utf-8")
            environment = json.loads((run / "environment.json").read_text(encoding="utf-8"))
            environment["lanes"] = receipt["snapshots"]
            (run / "environment.json").write_text(json.dumps(environment), encoding="utf-8")
            with self.assertRaisesRegex(IntegrityError, "silently omitted"):
                verify_run_evidence(run)
            receipt["test_matrix"].append({"lane": "linux-x11", "tests": []})
            (run / "receipt.json").write_text(json.dumps(receipt), encoding="utf-8")
            with self.assertRaisesRegex(IntegrityError, "empty requested lane"):
                verify_run_evidence(run)

    def test_receipt_rejects_duplicate_requested_lanes(self) -> None:
        value = {
            "schema_version": 1, "result": "fail", "lanes": ["linux-x11", "linux-x11"],
            "artifacts": [], "snapshots": {},
            "run_id": "unused", "source": {}, "commit_sha": "", "dirty_state_sha256": "",
            "artifact_manifest_sha256": "", "test_matrix": [], "durations_ms": {}, "teardown": {},
            "evidence_manifest_sha256": "",
        }
        with self.assertRaisesRegex(IntegrityError, "at least one lane"):
            validate_receipt(value)

    def test_linux_probe_uses_generic_deb_adapter_contract(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            artifact = Path(directory) / "probe.deb"
            build_probe_package(Path(__file__).resolve().parents[4], artifact)
            plan = inspect_linux_artifact(artifact)
            self.assertEqual(plan.kind, "deb-v1")
            self.assertEqual(plan.package_name, "sniptale-e2e-probe")
            self.assertEqual(plan.executable, "/usr/bin/sniptale-e2e-probe")

    def test_appimage_uses_portable_adapter_without_runner_changes(self) -> None:
        plan = inspect_linux_artifact(Path("Sniptale.AppImage"))
        self.assertEqual(plan.kind, "appimage-v1")
        self.assertEqual(plan.executable, "./artifact.AppImage")

    def test_browser_probe_is_deterministic_and_has_expected_extension_identity(self) -> None:
        root = Path(__file__).resolve().parents[4]
        with tempfile.TemporaryDirectory() as directory:
            first = build_browser_probe(root, Path(directory) / "probe-one.zip")
            second = build_browser_probe(root, Path(directory) / "probe-two.zip")
            self.assertEqual(first.read_bytes(), second.read_bytes())
            with zipfile.ZipFile(first) as archive:
                manifest = json.loads(archive.read("manifest.json"))
                self.assertEqual(
                    set(archive.namelist()), {"manifest.json", "probe.html", "probe.js"},
                )
                probe = archive.read("probe.js")
                self.assertIn(b"0" * 64, probe)
                self.assertNotIn(b"__SNIPTALE_E2E_NONCE__", probe)
            digest = hashlib.sha256(base64.b64decode(manifest["key"])).digest()[:16]
            extension_id = "".join(chr(ord("a") + nibble) for byte in digest for nibble in (byte >> 4, byte & 0xF))
            self.assertEqual(extension_id, "elhndangpenapadjimfoodlaenpcndpo")

    def test_linux_probe_package_is_byte_deterministic(self) -> None:
        root = Path(__file__).resolve().parents[4]
        with tempfile.TemporaryDirectory() as directory:
            first = build_probe_package(root, Path(directory) / "probe-one.deb")
            second = build_probe_package(root, Path(directory) / "probe-two.deb")
            self.assertEqual(first.read_bytes(), second.read_bytes())
            content = first.read_bytes()
            offset = 8
            members: dict[str, bytes] = {}
            while offset < len(content):
                header = content[offset:offset + 60]
                name = header[:16].decode("ascii").strip().rstrip("/")
                size = int(header[48:58].decode("ascii").strip())
                start = offset + 60
                members[name] = content[start:start + size]
                offset = start + size + (size % 2)
            with tarfile.open(fileobj=io.BytesIO(members["data.tar.gz"]), mode="r:gz") as data:
                self.assertTrue(data.getmember("./usr/lib/sniptale-e2e-probe").isdir())

    def test_windows_checksum_is_validated_before_cache_path_construction(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            paths = LabPaths.discover(Path(directory) / "repo")
            paths.ensure()
            environment = {
                "SNIPTALE_E2E_WINDOWS_ISO": "/does/not/matter.iso",
                "SNIPTALE_E2E_WINDOWS_ISO_SHA256": "../outside",
            }
            with mock.patch.dict(os.environ, environment, clear=False):
                with self.assertRaisesRegex(IntegrityError, "64 hexadecimal"):
                    Provisioner(paths, mock.Mock())._windows(mock.Mock())
            self.assertEqual(list(paths.images.iterdir()), [])

    def test_source_bundle_includes_dirty_files_and_excludes_state_secrets_and_media(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            root.mkdir()
            (root / "tracked.rs").write_text("fn main() {}", encoding="utf-8")
            (root / "src/install").mkdir(parents=True)
            (root / "src/install/pipe_secret.rs").write_text("pub struct PipeSecret;", encoding="utf-8")
            (root / "dirty-new.txt").write_text("dirty", encoding="utf-8")
            (root / "LICENSE").write_text("license", encoding="utf-8")
            (root / "NOTICE").write_text("notice", encoding="utf-8")
            (root / "opaque").write_text("excluded", encoding="utf-8")
            (root / "untracked-secret.txt").write_text("private", encoding="utf-8")
            (root / ".npmrc").write_text("//registry/:_authToken=private", encoding="utf-8")
            (root / "opaque.bin").write_bytes(b"private-media")
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            subprocess.run(
                ["git", "add", "tracked.rs", "src/install/pipe_secret.rs"], cwd=root, check=True,
            )
            (root / ".tmp").mkdir()
            (root / ".tmp/state.json").write_text("state", encoding="utf-8")
            (root / "signing.pfx").write_bytes(b"secret")
            (root / "recording.mp4").write_bytes(b"media")
            (root / "desktop-shot.png").write_bytes(b"media")
            (root / "resources").mkdir()
            (root / "resources/icon.png").write_bytes(b"synthetic-resource")
            archive = Path(directory) / "source.tar.gz"
            fingerprint = create_source_bundle(root, archive)
            second = Path(directory) / "source-second.tar.gz"
            create_source_bundle(root, second)
            with tarfile.open(archive, "r:gz") as bundle:
                names = set(bundle.getnames())
            self.assertEqual(names, {"LICENSE", "NOTICE", "dirty-new.txt", "resources/icon.png", "src/install/pipe_secret.rs", "tracked.rs"})
            self.assertEqual(fingerprint.file_count, 6)
            self.assertEqual(archive.read_bytes(), second.read_bytes())

    def test_source_bundle_rejects_escaping_directory_symlink(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            outside = Path(directory) / "outside"
            root.mkdir()
            outside.mkdir()
            (root / "linked").symlink_to(outside, target_is_directory=True)
            with self.assertRaisesRegex(IntegrityError, "symlink escapes"):
                create_source_bundle(root, Path(directory) / "source.tar.gz")

    def test_source_bundle_enforces_uncompressed_file_quota(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            root.mkdir()
            (root / "oversize.rs").write_text("too large", encoding="utf-8")
            with mock.patch("artifacts.MAX_SOURCE_FILE_BYTES", 1):
                with self.assertRaisesRegex(IntegrityError, "source file exceeds"):
                    create_source_bundle(root, Path(directory) / "source.tar.gz")

    def test_redaction_filters_token_extension_and_sensitive_path(self) -> None:
        value = "Bearer abc.def token=hunter2 chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa/ /home/alice/private/file"
        redacted = redact(value)
        self.assertNotIn("hunter2", redacted)
        self.assertNotIn("chrome-extension", redacted)
        self.assertNotIn("/home/alice", redacted)
        structured = redact_data({"nested": {"api_key": "raw-value", "safe": "ok"}})
        self.assertEqual(structured["nested"]["api_key"], "[redacted]")
        self.assertEqual(structured["nested"]["safe"], "ok")

    def test_redaction_preserves_safe_scenario_enums_but_not_recording_ids(self) -> None:
        safe = '\"scenario\":\"recording-system-audio\" recording-microphone'
        self.assertEqual(redact(safe), safe)
        self.assertEqual(redact("recording-system-audio-secret"), "[redacted]")
        self.assertEqual(redact("recording-secret-9"), "[redacted]")

    def test_redaction_covers_repository_security_corpus(self) -> None:
        corpus = json.loads(
            (Path(__file__).resolve().parents[4] / "contracts/security/redaction-corpus.json").read_text(encoding="utf-8")
        )
        forbidden = (
            "secret-token", "example.invalid", "AAAA_SECRET_MEDIA_BYTES", "password123",
            "copied-secret", "session=secret", "Alice", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "recording-secret-9", "capture-secret-9",
        )
        for sample in corpus["samples"]:
            value = redact(sample["value"])
            for secret in forbidden:
                self.assertNotIn(secret, value, sample["name"])

    def test_guest_evidence_rejects_symlink_escape(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "guest"
            root.mkdir()
            outside = Path(directory) / "outside.png"
            outside.write_bytes(b"not guest evidence")
            (root / "desktop.png").symlink_to(outside)
            with self.assertRaisesRegex(Exception, "unsafe file"):
                LaneRunner._validate_guest_tree(root)

    def test_remote_guest_evidence_is_rejected_before_transfer(self) -> None:
        ssh = mock.Mock()
        ssh.command.return_value = SimpleNamespace(stdout="e2e-out/escape-link\n")
        runner = object.__new__(LaneRunner)
        runner.config = SimpleNamespace(limits=SimpleNamespace(
            max_log_mib=1, max_screenshot_mib=1, max_recording_mib=1,
        ))
        with self.assertRaisesRegex(Exception, "unsafe file"):
            runner._enforce_remote_evidence_limit(ssh, windows=False)

    def test_teardown_evidence_accumulates_build_and_test_phases(self) -> None:
        runner = object.__new__(LaneRunner)
        runner.teardowns = {}
        runner._record_teardown("windows-11", "build", False, True)
        runner._record_teardown("windows-11", "test", True, False)
        evidence = runner.teardowns["windows-11"]
        self.assertTrue(evidence["build_post_forced"])
        self.assertTrue(evidence["test_pre_forced"])
        self.assertTrue(evidence["pre_run_forced"])
        self.assertTrue(evidence["post_run_forced"])

    def test_linux_teardown_requests_guest_poweroff_before_reset(self) -> None:
        runner = object.__new__(LaneRunner)
        runner.paths = mock.Mock()
        runner.vms = mock.Mock()
        runner.vms.current.return_value = mock.Mock()
        runner.config = SimpleNamespace(lanes={
            "linux-wayland": SimpleNamespace(os="linux", ssh_port=22421),
        })
        with mock.patch("runner.GuestSsh") as guest_ssh:
            runner._request_guest_shutdown("linux-wayland")
        guest_ssh.return_value.command.assert_called_once_with(
            ["sudo", "systemctl", "poweroff"], timeout=20,
        )

    def test_evidence_classification_keeps_lane_names_distinct(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            run = Path(directory)
            (run / "screenshots").mkdir()
            (run / "recordings").mkdir()
            for lane, content in (("linux-wayland", b"wayland"), ("linux-x11", b"x11")):
                guest = run / "guest" / lane / "evidence"
                guest.mkdir(parents=True)
                (guest / "desktop.png").write_bytes(content)
                LaneRunner._classify_evidence(run / "guest" / lane, run)
            self.assertEqual((run / "screenshots/linux-wayland-evidence-desktop.png").read_bytes(), b"wayland")
            self.assertEqual((run / "screenshots/linux-x11-evidence-desktop.png").read_bytes(), b"x11")

    def test_declared_guest_artifacts_must_match_transferred_files(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "desktop.png").write_bytes(b"synthetic")
            result = mock.Mock(artifacts=("missing.png",))
            with self.assertRaisesRegex(IntegrityError, "inventory does not match"):
                LaneRunner._verify_declared_artifacts(root, result)

    def test_guest_results_reject_skip_missing_and_corruption(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "result.json"
            base = {"schema_version": 1, "lane": "linux-x11", "environment": {}, "artifacts": []}
            base["tests"] = [{"name": "session", "status": "skip", "duration_ms": 1, "detail": ""}]
            path.write_text(json.dumps(base), encoding="utf-8")
            skipped = parse_guest_result(path, expected_lane="linux-x11", required_cases={"session"})
            self.assertFalse(skipped.passed)
            self.assertEqual(skipped.cases[0].status, "skip")
            base["tests"] = [{"name": "session", "status": "pass", "duration_ms": 1, "detail": ""}]
            path.write_text(json.dumps(base), encoding="utf-8")
            with self.assertRaisesRegex(IntegrityError, "silently omitted"):
                parse_guest_result(path, expected_lane="linux-x11", required_cases={"session", "portal"})
            path.write_text("not-json", encoding="utf-8")
            with self.assertRaisesRegex(IntegrityError, "invalid JSON"):
                parse_guest_result(path, expected_lane="linux-x11", required_cases={"session"})

    def test_receipt_schema_requires_all_identity_fields(self) -> None:
        receipt = {
            "schema_version": 1,
            "run_id": "20260713T120000Z",
            "source": {"sha256": "a" * 64},
            "commit_sha": "b" * 40,
            "dirty_state_sha256": "c" * 64,
            "artifact_manifest_sha256": "d" * 64,
            "evidence_manifest_sha256": "e" * 64,
            "artifacts": [{"name": "probe.deb", "sha256": "f" * 64, "bytes": 1}],
            "lanes": ["linux-x11"],
            "snapshots": {"linux-x11": {"disk_sha256": "f" * 64}},
            "test_matrix": [],
            "durations_ms": {"total": 1},
            "teardown": {"linux-x11": {"post_run_forced": False}},
            "result": "pass",
        }
        validate_receipt(receipt)
        receipt["run_mode"] = "alpha-release-acceptance"
        validate_receipt(receipt)
        receipt["run_mode"] = "invented-mode"
        with self.assertRaisesRegex(IntegrityError, "run mode"):
            validate_receipt(receipt)
        receipt["run_mode"] = "alpha-release-acceptance"
        del receipt["source"]
        with self.assertRaisesRegex(IntegrityError, "missing fields"):
            validate_receipt(receipt)


if __name__ == "__main__":
    unittest.main()
