from __future__ import annotations

import importlib.util
import json
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tooling.qa.runtime import compilation_cache

ROOT = Path(__file__).resolve().parents[3]


class CompilationCacheTests(unittest.TestCase):
    def test_cleans_application_in_host_windows_and_coverage_before_reuse(self):
        with mock.patch.object(compilation_cache.subprocess, "run") as run:
            self.assertEqual(compilation_cache.main(), 0)
        commands = [call.args[0] for call in run.call_args_list]
        self.assertEqual(len(commands), 3)
        for command in commands:
            self.assertEqual(command[:5], ["cargo", "clean", "--locked", "--package", "sniptale-native-agent"])
        self.assertEqual(
            [c[c.index("--target-dir") + 1] for c in commands], ["target", "target", "target/llvm-cov-target"]
        )
        self.assertIn("x86_64-pc-windows-msvc", commands[1])
        self.assertTrue(all(call.kwargs["check"] for call in run.call_args_list))

    def test_cleanup_failure_blocks_further_steps(self):
        with mock.patch.object(
            compilation_cache.subprocess, "run", side_effect=subprocess.CalledProcessError(1, "cargo")
        ) as run:
            with self.assertRaises(subprocess.CalledProcessError):
                compilation_cache.main()
            self.assertEqual(run.call_count, 1)

    def test_hosted_cache_rejects_unsafe_scope_skips_and_order(self):
        spec = importlib.util.spec_from_file_location("linux_cache_policy", ROOT / "tooling/qa/rules/verify_tooling.py")
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        for mutation in ("key", "restore-keys", "path", "order", "missing", "conditional", "nonblocking", "image"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                shutil.copytree(ROOT / ".github", root / ".github")
                config = root / "tooling/configs/qa"
                config.mkdir(parents=True)
                shutil.copyfile(ROOT / "tooling/configs/qa/toolchain-lock.json", config / "toolchain-lock.json")
                script = root / "tooling/release/windows/alpha-package.ps1"
                script.parent.mkdir(parents=True)
                shutil.copyfile(ROOT / script.relative_to(root), script)
                workflow = root / ".github/workflows/checks.yml"
                value = json.loads(workflow.read_text())
                steps = value["jobs"]["qa"]["steps"]
                cache = next(s for s in steps if s.get("name") == "Cache compiled Linux dependencies")
                invalidate = next(s for s in steps if s.get("name") == "Invalidate cached application outputs")
                if mutation in {"key", "restore-keys", "path"}:
                    cache["with"][mutation] = "unscoped"
                elif mutation == "order":
                    steps.remove(invalidate)
                    steps.append(invalidate)
                elif mutation == "missing":
                    steps.remove(invalidate)
                elif mutation == "conditional":
                    invalidate["if"] = "false"
                elif mutation == "image":
                    next(s for s in steps if s.get("id") == "build-image")["id"] = "wrong-image"
                else:
                    invalidate["continue-on-error"] = True
                workflow.write_text(json.dumps(value))
                errors = []
                with mock.patch.object(module, "ROOT", root):
                    module.validate_hosted_qa_policy(errors)
                self.assertTrue(any("Linux" in error for error in errors), errors)
