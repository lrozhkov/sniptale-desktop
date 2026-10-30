from __future__ import annotations

import io
import json
import os
import sys
import tempfile
import time
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path
from types import SimpleNamespace
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from config import LANES, LabConfig
from errors import RunFailed
import main as lab_main_module
from main import gc as lab_gc, parser
from paths import LabPaths


class CliConfigTests(unittest.TestCase):
    def test_cli_parses_documented_commands(self) -> None:
        cases = (
            (["doctor"], "doctor"),
            (["bootstrap"], "bootstrap"),
            (["provision", "all"], "provision"),
            (["start", "windows-11"], "start"),
            (["build", "linux-x11", "--profile", "dev"], "build"),
            (["test", "linux-wayland", "--artifact", "probe.deb"], "test"),
            (["run", "--lane", "all", "--build", "current"], "run"),
            (["status", "--json"], "status"),
            (["collect", "20260713T120000Z"], "collect"),
            (["rerun-failed", "20260713T120000Z"], "rerun-failed"),
            (["reset", "linux-x11"], "reset"),
            (["stop", "--all"], "stop"),
            (["gc"], "gc"),
            (["destroy", "windows-11", "--yes"], "destroy"),
        )
        for arguments, expected in cases:
            with self.subTest(arguments=arguments):
                self.assertEqual(parser().parse_args(arguments).command, expected)

    def test_focused_test_case_selection_is_explicit(self) -> None:
        args = parser().parse_args([
            "test", "windows-11", "--artifact", "probe.msi",
            "--cases", "recording-system-audio,capture",
        ])
        self.assertEqual(args.cases, "recording-system-audio,capture")

    def test_failed_run_still_prints_machine_readable_identity(self) -> None:
        paths = SimpleNamespace(
            root=Path("/tmp/sniptale-e2e-test-root"),
            lab=Path("/tmp/sniptale-e2e-test-root/lab"),
            scoped=lambda path: path,
            locks=Path("/tmp/sniptale-e2e-test-locks"),
            ensure=mock.Mock(),
        )
        config = SimpleNamespace(expand_lane=lambda lane: [lane])
        output = io.StringIO()
        diagnostics = io.StringIO()
        failure = RunFailed(
            "20260717T120000Z",
            "run 20260717T120000Z contains failed guest tests",
            "full-acceptance",
        )
        with (
            mock.patch("main.LabPaths.discover", return_value=paths),
            mock.patch("main.LabConfig.load", return_value=config),
            mock.patch("main.LabLock"),
            mock.patch("main.VmManager"),
            mock.patch("main.execute_run", side_effect=failure),
            redirect_stdout(output),
            redirect_stderr(diagnostics),
        ):
            status = lab_main_module.main([
                "run", "--lane", "windows-11", "--build", "current",
            ])
        self.assertEqual(status, 1)
        self.assertEqual(
            json.loads(output.getvalue()),
            {
                "run_id": "20260717T120000Z",
                "result": "fail",
                "mode": "full-acceptance",
            },
        )
        self.assertIn("failed guest tests", diagnostics.getvalue())

    def test_config_enforces_lane_resource_ceilings(self) -> None:
        root = Path(__file__).resolve().parents[4]
        config = LabConfig.load(root)
        self.assertEqual(set(config.lanes), set(LANES))
        self.assertLessEqual(config.lanes["windows-11"].memory_mib, 8192)
        self.assertLessEqual(config.lanes["linux-wayland"].memory_mib, 6144)
        self.assertTrue(all(lane.vcpus <= 4 for lane in config.lanes.values()))

    def test_scoped_deletion_rejects_outside_and_lab_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            root.mkdir()
            paths = LabPaths.discover(root)
            paths.ensure()
            with self.assertRaisesRegex(Exception, "outside disposable lab scope"):
                paths.scoped(root / "keep")
            with self.assertRaisesRegex(Exception, "lab root"):
                paths.scoped(paths.lab)
            self.assertEqual(paths.scoped(paths.runtime / "linux-x11"), paths.runtime / "linux-x11")

    def test_lab_root_symlink_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            outside = Path(directory) / "outside"
            (root / ".tmp/qa").mkdir(parents=True)
            outside.mkdir()
            (root / ".tmp/qa/desktop-e2e-lab").symlink_to(outside, target_is_directory=True)
            with self.assertRaisesRegex(Exception, "must not be redirected"):
                LabPaths.discover(root).ensure()

    def test_owned_lab_directory_symlink_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            outside = Path(directory) / "outside"
            paths = LabPaths.discover(root)
            paths.lab.mkdir(parents=True)
            paths.artifacts.mkdir(parents=True)
            outside.mkdir()
            paths.cache.symlink_to(outside, target_is_directory=True)
            with self.assertRaisesRegex(Exception, "must not be a symlink"):
                paths.ensure()

    def test_legacy_persistent_state_moves_out_of_temporary_qa_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            paths = LabPaths.discover(root)
            legacy = paths.lab / "golden/windows-11"
            legacy.mkdir(parents=True)
            (legacy / "identity.json").write_text("{}", encoding="utf-8")

            paths.ensure()

            self.assertFalse((paths.lab / "golden").exists())
            self.assertTrue((paths.golden / "windows-11/identity.json").is_file())
            self.assertTrue(paths.golden.is_relative_to(root / ".artifacts"))

    def test_gc_removes_abandoned_run_but_keeps_complete_receipt(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            config_dir = root / "tooling/desktop/e2e_lab"
            config_dir.mkdir(parents=True)
            source = Path(__file__).resolve().parents[1] / "lab-config.json"
            (config_dir / "lab-config.json").write_bytes(source.read_bytes())
            paths = LabPaths.discover(root)
            paths.ensure()
            abandoned = paths.lab / "20260713T120000Z"
            complete = paths.lab / "20260713T120001Z"
            abandoned.mkdir()
            complete.mkdir()
            (complete / "receipt.json").write_text("{}", encoding="utf-8")
            result = lab_gc(paths, LabConfig.load(root))
            self.assertEqual(result["incomplete_runs_removed"], 1)
            self.assertFalse(abandoned.exists())
            self.assertTrue(complete.exists())

    def test_gc_keeps_only_latest_transient_build_run_per_lane_and_profile(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            config_dir = root / "tooling/desktop/e2e_lab"
            config_dir.mkdir(parents=True)
            source = Path(__file__).resolve().parents[1] / "lab-config.json"
            (config_dir / "lab-config.json").write_bytes(source.read_bytes())
            paths = LabPaths.discover(root)
            paths.ensure()
            older = paths.lab / "20260713T120000Z"
            newer = paths.lab / "20260713T120001Z"
            acceptance = paths.lab / "20260713T120002Z"
            build_receipt = {
                "test_matrix": [
                    {
                        "lane": "windows-11",
                        "tests": [{"name": "build-release", "status": "pass"}],
                    }
                ]
            }
            for path in (older, newer):
                path.mkdir()
                (path / "receipt.json").write_text(json.dumps(build_receipt), encoding="utf-8")
                (path / "redundant.msi").write_bytes(b"staged")
            acceptance.mkdir()
            (acceptance / "receipt.json").write_text(
                json.dumps(
                    {
                        "test_matrix": [
                            {
                                "lane": "windows-11",
                                "tests": [{"name": "recording-system-audio", "status": "pass"}],
                            }
                        ]
                    }
                ),
                encoding="utf-8",
            )
            stamp = time.time()
            os.utime(older, (stamp - 2, stamp - 2))
            os.utime(newer, (stamp - 1, stamp - 1))
            os.utime(acceptance, (stamp, stamp))

            result = lab_gc(paths, LabConfig.load(root))

            self.assertEqual(result["redundant_build_runs_removed"], 1)
            self.assertFalse(older.exists())
            self.assertTrue(newer.exists())
            self.assertTrue(acceptance.exists())


if __name__ == "__main__":
    unittest.main()
