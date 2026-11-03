from __future__ import annotations

import json
import os
import subprocess
import tempfile
import unittest
import unittest.mock as mock
from pathlib import Path

from tooling.qa.runtime.analysis_tools import sha256
from tooling.qa.runtime.static_analysis import (
    RAW_REPORTS,
    ROOT,
    scan_inputs,
    source_inventory,
    source_policy_errors,
    verify_raw,
)


class WebSourceInventoryTests(unittest.TestCase):
    def test_shared_ui_is_analyzed_but_dependencies_and_built_assets_are_not(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            files = [
                "src/main.rs",
                "ui/src/panel.tsx",
                "ui/src/client.ts",
                "ui/tests/browser.mjs",
                "ui/node_modules/dependency/index.ts",
                "ui/dist/assets/bundle.js",
                ".tmp/probe.py",
            ]
            for name in files:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("const probe = 1;\n")
            selected = source_inventory(files, root)
            self.assertEqual(set(selected), set(files[:4]))
            (root / "ui/src/panel.tsx").write_text("// jscpd:ignore-start\n")
            self.assertTrue(source_policy_errors(root, selected))
            (root / "ui/src/client.ts").unlink()
            (root / "outside.ts").write_text("outside")
            (root / "ui/src/client.ts").symlink_to(root / "outside.ts")
            with self.assertRaises(ValueError):
                source_inventory(files, root)


class ReclassificationTests(unittest.TestCase):
    def test_wrapper_accepts_only_explicit_review_syntax_and_records_its_mode(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            wrapper = root / "tooling/qa/wrappers/static-analysis.sh"
            wrapper.parent.mkdir(parents=True)
            wrapper.write_bytes((ROOT / "tooling/qa/wrappers/static-analysis.sh").read_bytes())
            library = root / "tooling/qa/runtime/lib.sh"
            library.parent.mkdir()
            library.write_text('ensure_rust_env() { :; }\nqa_init() { printf "%s\\n" "$3"; exit 0; }\n')
            for arguments, mode in (([], "default"), (["--review-run", "run-123"], "review-existing-results")):
                result = subprocess.run(["bash", str(wrapper), *arguments], capture_output=True, text=True)
                self.assertEqual((result.returncode, result.stdout.strip()), (0, mode))
            for arguments in (["--review-run=run-123"], ["--review-r", "run-123"], ["--review-run"], ["x"]):
                result = subprocess.run(["bash", str(wrapper), *arguments], capture_output=True, text=True)
                self.assertEqual((result.returncode, result.stdout), (2, ""))

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.parent = Path(self.temporary.name)
        self.root = self.parent / "repo"
        self.root.mkdir()
        subprocess.run(["git", "init", "-q"], cwd=self.root, check=True)
        self.source = self.root / "source.py"
        self.source.write_text("value = 1\n")
        self.readme = self.root / "README.md"
        self.readme.write_text("Documentation.\n")
        self.registry = self.root / "tooling/configs/qa/exception-registry.json"
        self.registry.parent.mkdir(parents=True)
        self.registry.write_text("{}\n")
        self.tool_pin = self.registry.with_name("tool-versions.json")
        self.tool_pin.write_text('{"tool": "1.0.0"}\n')
        subprocess.run(["git", "add", "."], cwd=self.root, check=True)
        subprocess.run(
            ["git", "-c", "user.name=QA", "-c", "user.email=qa@example.invalid", "commit", "-qm", "fixture"],
            cwd=self.root,
            check=True,
        )

    def test_only_documentation_and_registry_changes_preserve_raw_inputs(self) -> None:
        before = scan_inputs(self.root)
        self.registry.write_text('{"reviewed": true}\n')
        self.readme.write_text("Revised documentation.\n")
        self.assertEqual(scan_inputs(self.root), before)
        self.source.write_text("value = 2\n")
        self.assertNotEqual(scan_inputs(self.root), before)
        self.source.write_text("value = 1\n")
        with mock.patch.dict(os.environ, {"RUSTFLAGS": "--cfg changed"}):
            self.assertNotEqual(scan_inputs(self.root), before)
        self.tool_pin.write_text('{"tool": "2.0.0"}\n')
        self.assertNotEqual(scan_inputs(self.root), before)
        self.tool_pin.write_text('{"tool": "1.0.0"}\n')
        (self.root / "new.py").write_text("new = True\n")
        self.assertNotEqual(scan_inputs(self.root), before)

    def test_complete_raw_reports_reject_tampering_missing_reports_and_source_drift(self) -> None:
        output = self.parent / "report"
        output.mkdir()
        inputs = scan_inputs(self.root)
        for name in RAW_REPORTS:
            (output / name).write_text("{}\n")
        manifest = {
            "schemaVersion": 1,
            "kind": "sniptale-analysis-scan",
            "scanInputs": inputs,
            "reports": {name: sha256(output / name) for name in RAW_REPORTS},
            "metrics": [],
        }
        (output / "scan.json").write_text(json.dumps(manifest))
        self.assertEqual(verify_raw(output, inputs), manifest)
        for name in ("CODEQL_EXTRACTOR_PYTHON_EXTRACT_STDLIB", "CARGO_HOME", "CARGO_PROFILE_DEV_DEBUG"):
            with mock.patch.dict(os.environ, {name: "changed"}), self.assertRaisesRegex(ValueError, "inputs changed"):
                verify_raw(output, scan_inputs(self.root))
        self.source.write_text("value = 2\n")
        with self.assertRaisesRegex(ValueError, "inputs changed"):
            verify_raw(output, scan_inputs(self.root))
        report = output / sorted(RAW_REPORTS)[0]
        report.write_text("changed\n")
        with self.assertRaisesRegex(ValueError, "missing or changed"):
            verify_raw(output, inputs)
        report.unlink()
        with self.assertRaisesRegex(ValueError, "missing or changed"):
            verify_raw(output, inputs)
        manifest["reports"].pop(sorted(RAW_REPORTS)[0])
        (output / "scan.json").write_text(json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, "inventory is incomplete"):
            verify_raw(output, inputs)
