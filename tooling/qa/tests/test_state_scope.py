from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path
import unittest.mock as mock

from tooling.qa.runtime import repository, state


class ScopeTests(unittest.TestCase):
    def test_classifies_harness_and_shared_control_targets(self) -> None:
        self.assertTrue(repository.is_harness_target("tooling/qa/runtime/run.py"))
        self.assertFalse(repository.is_harness_target("docs/tooling/qa.md"))
        self.assertTrue(repository.is_harness_target("AGENTS.md"))
        self.assertTrue(repository.is_harness_target(".agents/skills/architecture-code-review/SKILL.md"))
        self.assertTrue(repository.is_harness_target(".github/workflows/ci.yml"))
        self.assertTrue(repository.is_harness_target(".config/nextest.toml"))
        self.assertTrue(repository.is_harness_target("rustfmt.toml"))
        self.assertFalse(repository.is_harness_target("src/app/host.rs"))

    def test_scope_selection_preserves_guards_and_fails_closed(self) -> None:
        cases = {
            "README.md": {"docs"},
            "AGENTS.md": {"docs", "scripts"},
            ".agents/skills/architecture-code-review/SKILL.md": {"docs", "scripts"},
            "ui/src/App.tsx": {"docs", "ui"},
            "tooling/release/windows/alpha_publish.py": {"docs", "scripts"},
            "src/audio/device.rs": {"docs", "rust"},
            "src/ipc/frame.rs": {"docs", "rust", "long-tests"},
        }
        for path, expected in cases.items():
            with self.subTest(path=path):
                self.assertEqual(set(repository.check_plan([path])), expected)
        for path in (
            "unknown.bin",
            "tooling/qa/runtime/state.py",
            ".github/workflows/checks.yml",
            ".config/nextest.toml",
        ):
            self.assertIn("full", repository.check_plan([path]))
        self.assertIn("dependencies", repository.check_plan(["Cargo.lock"]))
        self.assertEqual(set(repository.check_plan(["README.md", "ui/src/App.tsx"])), {"docs", "ui"})
        self.assertIn("full", repository.check_plan(["README.md"], full=True))
        self.assertEqual(repository.harness_targets([]), [])

    def test_pull_request_scope_includes_committed_changes_and_rejects_bad_base(self) -> None:
        with mock.patch.dict("os.environ", {"SNIPTALE_QA_BASE": "a" * 40}):
            with mock.patch.object(
                repository, "_git", side_effect=[b"local.md\0", b"b" * 40, b"ui/src/App.tsx\0", b""]
            ):
                self.assertEqual(repository.current_targets(), ["local.md", "ui/src/App.tsx"])
            with (
                mock.patch.object(repository, "_git", side_effect=[b"", RuntimeError("missing base")]),
                self.assertRaisesRegex(RuntimeError, "missing base"),
            ):
                repository.current_targets()
        with (
            mock.patch.dict("os.environ", {"SNIPTALE_QA_BASE": "main"}),
            self.assertRaisesRegex(RuntimeError, "full commit SHA"),
        ):
            repository.current_targets()

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
                original_mode = (root / "new.md").stat().st_mode
                (root / "new.md").chmod(0o755)
                self.assertNotEqual(repository.fingerprint(), before)
                (root / "new.md").chmod(original_mode)
                subprocess.run(["git", "add", "-A"], cwd=root, check=True)
                self.assertEqual(repository.current_targets(), ["new.md", "old.md"])
                self.assertEqual(repository.fingerprint(), before)


class StateTests(unittest.TestCase):
    def test_checkpoint_rejects_input_changes_before_writing_success(self) -> None:
        source = {"revision": "a" * 40, "targetFiles": ["src/main.rs"], "fingerprint": "1" * 64}
        with (
            mock.patch.object(state, "read_json", return_value={"repository": source}),
            mock.patch.object(state, "current_targets", return_value=["src/main.rs"]),
            mock.patch.object(state, "revision", return_value="a" * 40),
            mock.patch.object(state, "fingerprint", return_value="1" * 64),
        ):
            state.assert_run_inputs(Path("fixture.json"))
            for field, value in (("revision", "b" * 40), ("current_targets", []), ("fingerprint", "2" * 64)):
                with (
                    self.subTest(field=field),
                    mock.patch.object(state, field, return_value=value),
                    self.assertRaisesRegex(RuntimeError, "changed during proof"),
                ):
                    state.assert_run_inputs(Path("fixture.json"))

    def test_reuse_rejects_changed_flags_plan_frontend_and_failed_state(self) -> None:
        import json

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            frontend = root / "ui/dist/index.html"
            frontend.parent.mkdir(parents=True)
            frontend.write_text("original")
            with (
                mock.patch.object(repository, "ROOT", root),
                mock.patch.object(repository, "revision", return_value="a" * 40),
                mock.patch.object(state, "current_targets", return_value=[]),
                mock.patch.object(state, "revision", return_value="a" * 40),
                mock.patch.object(state, "check_plan", return_value=["docs", "ui"]),
            ):
                state.write_state(root, "checkpoint", "passed-run")
                state.assert_state(root, "checkpoint")
                with (
                    mock.patch.dict("os.environ", {"NODE_OPTIONS": "--max-old-space-size=100"}),
                    self.assertRaisesRegex(RuntimeError, "context"),
                ):
                    state.assert_state(root, "checkpoint")
                with (
                    mock.patch.object(state, "check_plan", return_value=["docs"]),
                    self.assertRaisesRegex(RuntimeError, "context"),
                ):
                    state.assert_state(root, "checkpoint")
                frontend.write_text("modified")
                with self.assertRaisesRegex(RuntimeError, "context"):
                    state.assert_state(root, "checkpoint")
                frontend.write_text("original")
                path = state.state_path(root, "checkpoint")
                proof = json.loads(path.read_text())
                proof["status"] = "failed"
                path.write_text(json.dumps(proof))
                with self.assertRaisesRegex(RuntimeError, "status"):
                    state.assert_state(root, "checkpoint")
                path.unlink()
                with self.assertRaisesRegex(RuntimeError, "missing"):
                    state.assert_state(root, "checkpoint")

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
                self.assertRaisesRegex(RuntimeError, "fingerprint"),
            ):
                state.assert_state(root, "release-harness")


if __name__ == "__main__":
    unittest.main()
