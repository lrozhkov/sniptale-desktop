from __future__ import annotations

import json
import io
import os
import tempfile
import unittest
from contextlib import redirect_stderr
from datetime import datetime, timedelta, timezone
from pathlib import Path
from unittest import mock

from tooling.qa.runtime import lock, run, storage


class StorageTests(unittest.TestCase):
    def test_sanitizes_paths_secrets_and_bounds_logs(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            log = root / ".tmp/qa-logs/2026-01-01/run.log"
            storage.append_bounded_log(
                log,
                f"repo={root} API_TOKEN=visible signed-secret\n" + ("x" * 300_000),
                root,
                ["signed-secret"],
            )
            text = log.read_text(encoding="utf-8")
            self.assertNotIn(str(root), text)
            self.assertNotIn("visible", text)
            self.assertNotIn("signed-secret", text)
            self.assertIn("<repo>", text)
            self.assertIn("log truncated", text)
            self.assertLessEqual(log.stat().st_size, storage.MAX_LOG_BYTES)

    def test_sanitizes_windows_user_paths(self) -> None:
        value = storage.sanitize_text(
            r"artifact=C:\Users\Alice\Private\capture.mp4",
            Path("/repo"),
        )
        self.assertNotIn("Alice", value)
        self.assertNotIn("capture.mp4", value)
        self.assertIn("<user-path>", value)

    def test_truncation_marker_replaces_a_tail_too_small_for_the_marker(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            log = root / ".tmp/qa-logs/2026-01-01/run.log"
            log.parent.mkdir(parents=True)
            log.write_bytes(b"x" * (storage.MAX_LOG_BYTES - 5))

            storage.append_bounded_log(log, "overflow", root)

            payload = log.read_bytes()
            self.assertEqual(len(payload), storage.MAX_LOG_BYTES)
            self.assertTrue(payload.endswith(storage.TRUNCATION_MARKER))
            metadata = storage.log_metadata(log, ".tmp/qa-logs/2026-01-01/run.log")
            self.assertTrue(metadata["truncated"])
            self.assertEqual(metadata["byteCount"], storage.MAX_LOG_BYTES)

    def test_sanitizes_authorization_cookie_and_api_key_assignments(self) -> None:
        value = storage.sanitize_text(
            "Authorization=Bearer-private Cookie=session-private API_KEY=api-private",
            Path("/repo"),
        )
        self.assertNotIn("Bearer-private", value)
        self.assertNotIn("session-private", value)
        self.assertNotIn("api-private", value)

    def test_sanitizes_security_redaction_corpus_from_qa_logs(self) -> None:
        corpus_path = (
            Path(__file__).resolve().parents[3] / "contracts/security/redaction-corpus.json"
        )
        corpus = json.loads(corpus_path.read_text(encoding="utf-8"))
        value = storage.sanitize_text(
            "\n".join(sample["value"] for sample in corpus["samples"]),
            Path("/repo"),
        )
        for forbidden in (
            "secret-token",
            "example.invalid",
            "AAAA_SECRET_MEDIA_BYTES",
            "password123",
            "copied-secret",
            "Bearer secret",
            "session=secret",
            "Alice",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "recording-secret-9",
            "screenshot-secret-9",
            "capture-secret-9",
        ):
            self.assertNotIn(forbidden, value)
        self.assertIn("<media-payload>", value)
        self.assertIn("<user-path>", value)
        self.assertIn("<redacted>", value)

    def test_private_files_reject_symlink_redirection(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            outside = root / "outside.txt"
            outside.write_text("preserve", encoding="utf-8")
            log = root / ".tmp/qa-logs/2026-01-01/run.log"
            log.parent.mkdir(parents=True)
            log.symlink_to(outside)
            with self.assertRaises(OSError):
                storage.append_bounded_log(log, "TOKEN=private\n", root)
            state = root / ".tmp/qa/state/checkpoint.json"
            state.parent.mkdir(parents=True)
            state.symlink_to(outside)
            with self.assertRaises(OSError):
                storage.atomic_write_json(state, {"status": "passed"})
            self.assertEqual(outside.read_text(encoding="utf-8"), "preserve")

    def test_atomic_json_is_private_and_round_trips(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "state/value.json"
            storage.atomic_write_json(path, {"status": "passed"})
            self.assertEqual(storage.read_json(path)["status"], "passed")
            if os.name != "nt":
                self.assertEqual(path.stat().st_mode & 0o777, 0o600)

    def test_retention_recovers_stale_runs_and_bounds_invalid_and_orphan_files(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            now = datetime(2026, 7, 17, 12, tzinfo=timezone.utc)
            day = "2026-07-17"
            stale_path = root / f".tmp/qa-observability/runs/{day}/stale.json"
            stale_log = root / f".tmp/qa-logs/{day}/stale.log"
            storage.append_bounded_log(stale_log, "started\n", root)
            storage.atomic_write_json(stale_path, {
                "schemaVersion": 1,
                "runId": "stale",
                "status": "running",
                "startedAt": (now - timedelta(hours=7)).isoformat().replace("+00:00", "Z"),
                "finishedAt": None,
                "durationMs": None,
                "exitCode": None,
                "problemIds": [],
                "log": {"path": f".tmp/qa-logs/{day}/stale.log"},
            })
            old_path = root / ".tmp/qa-observability/runs/2026-05-01/old.json"
            old_log = root / ".tmp/qa-logs/2026-05-01/old.log"
            storage.append_bounded_log(old_log, "old\n", root)
            storage.atomic_write_json(old_path, {
                "schemaVersion": 1,
                "runId": "old",
                "status": "all-passed",
                "startedAt": (now - timedelta(days=31)).isoformat().replace("+00:00", "Z"),
                "log": {"path": ".tmp/qa-logs/2026-05-01/old.log"},
            })
            invalid_root = root / ".tmp/qa-observability/runs/invalid"
            invalid_root.mkdir(parents=True)
            for index in range(30):
                (invalid_root / f"{index:02}.json").write_text("{broken", encoding="utf-8")
            orphan_root = root / ".tmp/qa-logs/orphans"
            orphan_root.mkdir(parents=True)
            for index in range(30):
                (orphan_root / f"{index:02}.log").write_text("orphan", encoding="utf-8")
            with mock.patch.object(storage, "utc_now", return_value=now):
                storage.maintain_storage(root)
            recovered = storage.read_json(stale_path)
            self.assertEqual(recovered["status"], "interrupted")
            self.assertEqual(recovered["exitCode"], 143)
            self.assertGreater(recovered["durationMs"], 0)
            self.assertTrue(recovered["log"]["digest"])
            self.assertFalse(old_path.exists())
            self.assertFalse(old_log.exists())
            self.assertLessEqual(len(list(invalid_root.glob("*.json"))), storage.MAX_INVALID)
            self.assertLessEqual(len(list(orphan_root.glob("*.log"))), storage.MAX_ORPHAN_LOGS)


class LockTests(unittest.TestCase):
    def test_rejects_concurrent_root_and_accepts_tokenized_child(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with mock.patch.dict(os.environ, {}, clear=True):
                token, owner = lock.acquire(root, "qa:checkpoint", "root-run", "root-run")
                self.assertTrue(owner)
                with self.assertRaisesRegex(RuntimeError, "already active"):
                    lock.acquire(root, "qa:build", "other-run", "other-run")
                with mock.patch.dict(
                    os.environ,
                    {"SNIPTALE_QA_LOCK_TOKEN": token},
                    clear=True,
                ):
                    inherited, child_owner = lock.acquire(
                        root, "qa:build", "child-run", "root-run"
                    )
                self.assertEqual(inherited, token)
                self.assertFalse(child_owner)
                lock.release(root, "child-run")
                self.assertTrue((root / lock.LOCK_RELATIVE_PATH).is_file())
                lock.release(root, "root-run")
                self.assertFalse((root / lock.LOCK_RELATIVE_PATH).exists())

    def test_recovers_stale_lock(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / lock.LOCK_RELATIVE_PATH
            storage.atomic_write_json(
                path,
                {
                    "pid": 999_999_999,
                    "wrapperId": "qa:stale",
                    "ownerRunId": "stale",
                    "rootRunId": "stale",
                    "tokenDigest": "invalid",
                },
            )
            with mock.patch.dict(os.environ, {}, clear=True):
                _token, owner = lock.acquire(root, "qa:new", "new", "new")
            self.assertTrue(owner)
            lock.release(root, "new")


class RunTests(unittest.TestCase):
    def test_live_step_output_is_redacted_before_streaming(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            record_path = root / "record.json"
            log_path = root / ".tmp/qa-logs/2026-07-17/run.log"
            storage.atomic_write_json(record_path, {
                "steps": [],
                "log": {"path": ".tmp/qa-logs/2026-07-17/run.log"},
            })
            streamed = io.StringIO()
            with mock.patch.object(run, "ROOT", root), redirect_stderr(streamed):
                code = run.run_step(
                    record_path,
                    log_path,
                    "qa.step.live",
                    "live",
                    ["python3", "-c", "print('TOKEN=private-value phase=build')"],
                    ["private-value"],
                    True,
                )
            self.assertEqual(code, 0)
            self.assertIn("phase=build", streamed.getvalue())
            self.assertNotIn("private-value", streamed.getvalue())

    def test_shell_step_output_is_sanitized_recorded_and_removed(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            record_path = root / "record.json"
            log_path = root / ".tmp/qa-logs/2026-07-17/run.log"
            output_path = root / "shell.raw"
            storage.atomic_write_json(record_path, {
                "steps": [],
                "log": {"path": ".tmp/qa-logs/2026-07-17/run.log"},
            })
            output_path.write_text("TOKEN=private-value\n", encoding="utf-8")
            with mock.patch.object(run, "ROOT", root):
                started_at, started_ns = run.begin_shell_step(log_path, "shell", "shell")
                code = run.finish_shell_step(
                    record_path, log_path, "shell", "shell", started_at, started_ns,
                    7, output_path, ["private-value"],
                )
            self.assertEqual(code, 7)
            self.assertFalse(output_path.exists())
            self.assertNotIn("private-value", log_path.read_text(encoding="utf-8"))
            self.assertEqual(storage.read_json(record_path)["steps"][0]["exitCode"], 7)

    def test_records_hierarchy_step_failure_and_sanitized_log(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with (
                mock.patch.object(run, "ROOT", root),
                mock.patch.object(run, "current_targets", return_value=["tooling/example.py"]),
                mock.patch.object(run, "fingerprint", return_value="f" * 64),
                mock.patch.object(run, "revision", return_value="a" * 40),
                mock.patch.dict(
                    os.environ,
                    {
                        "SNIPTALE_QA_RUN_ID": "parent",
                        "SNIPTALE_QA_ROOT_RUN_ID": "root",
                    },
                    clear=True,
                ),
            ):
                session = run.start("qa:test", "default", False, ["private-value"])
                code = run.run_step(
                    Path(session["run_path"]),
                    Path(session["log_path"]),
                    "qa.step.fake",
                    "fake",
                    ["python3", "-c", "print('private-value'); raise SystemExit(3)"],
                    ["private-value"],
                )
                self.assertEqual(code, 3)
                record = run.finish(
                    Path(session["run_path"]), Path(session["log_path"]), 3
                )
            self.assertEqual(record["rootRunId"], "root")
            self.assertEqual(record["parentRunId"], "parent")
            self.assertEqual(record["status"], "problems-found")
            self.assertEqual(record["steps"][0]["exitCode"], 3)
            self.assertNotIn(
                "private-value", Path(session["log_path"]).read_text(encoding="utf-8")
            )
            json.loads(Path(session["run_path"]).read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
