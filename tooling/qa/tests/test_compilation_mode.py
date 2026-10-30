from __future__ import annotations

import os
import subprocess
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]


class CompilationModeTests(unittest.TestCase):
    def test_defaults_and_explicit_override(self):
        for mode, supplied, expected in (
            ("dev", {}, "1"), ("release", {}, "0"),
            ("dev", {"CI": "true"}, "0"), ("dev", {"GITHUB_ACTIONS": "true"}, "0"),
            ("dev", {"CARGO_INCREMENTAL": "0"}, "0"),
            ("release", {"CARGO_INCREMENTAL": "1"}, "1"),
        ):
            with self.subTest(mode=mode, supplied=supplied):
                environment = {k: v for k, v in os.environ.items() if k not in {"CI", "GITHUB_ACTIONS", "CARGO_INCREMENTAL"}}
                environment.update(supplied)
                result = subprocess.run(
                    ["bash", "-c", 'source tooling/qa/runtime/lib.sh; ensure_rust_env "$1"; printf "%s" "$CARGO_INCREMENTAL"', "fixture", mode],
                    cwd=ROOT, env=environment, capture_output=True, text=True, check=True,
                )
                self.assertEqual(result.stdout, expected)
