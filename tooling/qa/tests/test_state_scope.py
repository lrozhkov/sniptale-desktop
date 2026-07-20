from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tooling.qa.runtime import repository, state


class ScopeTests(unittest.TestCase):
    def test_classifies_harness_and_shared_control_targets(self) -> None:
        self.assertTrue(repository.is_harness_target("tooling/qa/runtime/run.py"))
        self.assertTrue(repository.is_harness_target("docs/tooling/qa.md"))
        self.assertTrue(repository.is_harness_target("AGENTS.md"))
        self.assertTrue(repository.is_harness_target(".github/workflows/ci.yml"))
        self.assertTrue(repository.is_harness_target(".config/nextest.toml"))
        self.assertTrue(repository.is_harness_target("rustfmt.toml"))
        self.assertFalse(repository.is_harness_target("src/app/host.rs"))

    def test_fingerprint_is_stable_when_a_move_is_staged(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            subprocess.run(["git", "init", "-q"], cwd=root, check=True)
            (root / "old.md").write_text("content\n", encoding="utf-8")
            subprocess.run(["git", "add", "old.md"], cwd=root, check=True)
            subprocess.run(
                ["git", "-c", "user.name=QA", "-c", "user.email=qa@example.invalid", "commit", "-qm", "initial"],
                cwd=root,
                check=True,
            )
            (root / "old.md").rename(root / "new.md")
            with mock.patch.object(repository, "ROOT", root):
                before = repository.fingerprint()
                subprocess.run(["git", "add", "-A"], cwd=root, check=True)
                self.assertEqual(repository.current_targets(), ["new.md", "old.md"])
                self.assertEqual(repository.fingerprint(), before)


class StateTests(unittest.TestCase):
    def test_accepts_exact_state_and_rejects_fingerprint_drift(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            targets = ["tooling/qa/runtime/run.py"]
            with (
                mock.patch.object(state, "current_targets", return_value=targets),
                mock.patch.object(state, "harness_targets", side_effect=lambda value=None: targets),
                mock.patch.object(state, "revision", return_value="a" * 40),
                mock.patch.object(state, "fingerprint", return_value="1" * 64),
            ):
                state.write_state(root, "release-harness", "run-1")
                state.assert_state(root, "release-harness")
            with (
                mock.patch.object(state, "current_targets", return_value=targets),
                mock.patch.object(state, "harness_targets", side_effect=lambda value=None: targets),
                mock.patch.object(state, "revision", return_value="a" * 40),
                mock.patch.object(state, "fingerprint", return_value="2" * 64),
            ):
                with self.assertRaisesRegex(RuntimeError, "fingerprint"):
                    state.assert_state(root, "release-harness")


if __name__ == "__main__":
    unittest.main()
