from __future__ import annotations

import importlib.util
import json
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from unittest import mock

from tooling.qa.runtime import parallel, run, storage
from tooling.qa.runtime.checkpoint_checks import CHECKS
from tooling.qa.runtime.metrics import compare, load_run, summarize


class ParallelProofTests(unittest.TestCase):
    def test_runs_every_check_and_preserves_order_failure_and_redaction(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            record, log = root / "record.json", root / "run.log"
            storage.atomic_write_json(record, {"steps": [], "log": {"path": "run.log"}})
            checks = [("failure", [sys.executable, "-c", "print('TOKEN=secret-value'); raise SystemExit(7)"]),
                      ("missing", [str(root / "missing-executable")]),
                      ("success", [sys.executable, "-c", "print('complete')"])]
            with mock.patch.object(parallel, "ROOT", root), mock.patch.object(run, "ROOT", root):
                self.assertEqual(parallel.run_group(checks, record, log, 2), 7)
            steps = storage.read_json(record)["steps"]
            self.assertEqual([s["exitCode"] for s in steps], [7, 127, 0])
            self.assertEqual([s["label"] for s in steps], ["failure", "missing", "success"])
            self.assertNotIn("secret-value", log.read_text())
            self.assertIn("complete", log.read_text())
            self.assertEqual(list((root / ".tmp/qa-reports").iterdir()), [])

    def test_concurrency_is_bounded_and_workers_overlap(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            record, log = root / "record.json", root / "run.log"
            storage.atomic_write_json(record, {"steps": [], "log": {"path": "run.log"}})
            barrier = threading.Barrier(2, timeout=10)
            active = peak = 0
            mutex = threading.Lock()

            def fake_run(path, log_path, step_id, label, _command):
                nonlocal active, peak
                with mutex:
                    active += 1
                    peak = max(peak, active)
                barrier.wait()
                with mutex:
                    active -= 1
                log_path.write_text("completed\n")
                storage.atomic_write_json(path, {"steps": [{"label": label, "durationMs": 1, "exitCode": 0}]})
                return 0

            with mock.patch.object(parallel, "ROOT", root), mock.patch.object(parallel, "run_step", side_effect=fake_run):
                self.assertEqual(parallel.run_group([(str(i), ["fixture"]) for i in range(6)], record, log, 2), 0)
            self.assertEqual(peak, 2)
            self.assertEqual(len(storage.read_json(record)["steps"]), 6)

    def test_rejects_invalid_bounds_and_duplicate_labels(self):
        for workers in (0, -1, 5):
            with self.assertRaises(ValueError):
                parallel.run_group([("x", ["true"])], Path("unused"), Path("unused"), workers)
        with self.assertRaises(ValueError):
            parallel.run_group([("x", ["true"])] * 2, Path("unused"), Path("unused"), 1)

    def test_catalog_retains_all_twenty_independent_checks(self):
        self.assertEqual({label for label, _ in CHECKS}, {
            "fmt", "documentation policy", "open-source legal policy", "audit11 architecture snapshot",
            "tooling topology and exception registry", "architecture guardrail self-tests",
            "architecture guardrails", "topology guardrails", "boundary guardrail self-tests",
            "boundary guardrails", "contract guardrail self-tests", "contract guardrails",
            "release artifact guardrails", "Windows resource guardrails", "security guardrail self-tests",
            "security guardrails", "dev dependency guardrails", "test policy",
            "desktop E2E lab controller tests", "QA runtime tests",
        })
        self.assertEqual(sum("--self-test" in cmd for _, cmd in CHECKS), 4)
        self.assertEqual(sum("unittest" in cmd for _, cmd in CHECKS), 2)
        self.assertTrue(all(cmd[:2] != ["cargo", "check"] for _, cmd in CHECKS))

    def test_release_artifact_check_cannot_be_removed_from_the_catalog(self):
        root = Path(__file__).resolve().parents[3]
        spec = importlib.util.spec_from_file_location("catalog_guard", root / "tooling/qa/rules/verify_test_policy.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        for mutation in ("missing", "dynamic", "omitted"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                fixture = Path(directory)
                catalog = fixture / "tooling/qa/runtime/checkpoint_checks.py"
                catalog.parent.mkdir(parents=True)
                if mutation == "dynamic":
                    catalog.write_text("CHECKS = build_checks()\n")
                elif mutation == "omitted":
                    catalog.write_text("CHECKS = []\n")
                with mock.patch.object(module, "ROOT", fixture), self.assertRaises(SystemExit):
                    module.check_release_wrapper_contract()


class MetricsTests(unittest.TestCase):
    def test_malformed_step_or_source_is_rejected(self):
        for steps, source in (([None], {}), ([{"durationMs": True}], {}), ([], []), ([], {"revision": "https://example.invalid/private"})):
            with self.subTest(steps=steps, source=source), self.assertRaises(ValueError):
                summarize({"durationMs": 1, "steps": steps, "repository": source})

    def test_parallel_children_do_not_double_count_wall_time(self):
        receipt = {"runId": "fixture", "wrapperId": "qa:checkpoint", "durationMs": 100,
                   "steps": [{"label": "group", "durationMs": 80, "exitCode": 0},
                             {"label": "child", "durationMs": 60, "exitCode": 0, "parallelGroup": "group"}]}
        report = summarize(receipt)
        self.assertEqual(report["wallMs"], 100)
        self.assertEqual(report["topLevelStepMs"], 80)
        self.assertEqual(report["parallelWorkMs"], 60)
        self.assertEqual(compare({**report, "wallMs": 200}, report)["savedPercent"], 50)
        with self.assertRaises(ValueError):
            compare({**report, "wrapperId": "qa:build"}, report)

    def test_rejects_path_escape_incomplete_and_failed_receipts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for run_id in ("../outside", "", "/absolute"):
                with self.assertRaises(ValueError):
                    load_run(root, run_id)
            run_id = "20261030T120000.000000Z-aaaaaaaaaaaa"
            path = root / ".tmp/qa-observability/runs/2026-10-30" / f"{run_id}.json"
            path.parent.mkdir(parents=True)
            for status, code in (("running", None), ("problems-found", 1), ("all-passed", 1)):
                path.write_text(json.dumps({"runId": run_id, "status": status, "exitCode": code}))
                with self.assertRaises(ValueError):
                    load_run(root, run_id)


if __name__ == "__main__":
    unittest.main()
