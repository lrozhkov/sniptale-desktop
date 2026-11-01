"""Threshold, scope, warning and native pinned-Clippy regression proof."""

from __future__ import annotations

import contextlib
import io
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tooling.qa.rules import verify_boundaries as rules
from tooling.qa.runtime import run, storage


class CodeSizeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.src = self.root / "src"
        self.src.mkdir()
        for name, value in (("ROOT", self.root), ("SRC", self.src)):
            patch = mock.patch.object(rules, name, value)
            patch.start()
            self.addCleanup(patch.stop)

    def source(self, name, lines):
        path = self.src / name
        path.parent.mkdir(parents=True, exist_ok=True)
        # Physical size includes both comments and blank lines.
        path.write_text("// context\n" * (lines // 2) + "\n" * (lines - lines // 2))
        return path

    def findings(self, path, exceptions=()):
        errors, warnings = [], []
        rules.file_size_findings([path], list(exceptions), errors, warnings)
        return errors, warnings

    def test_physical_file_boundaries(self):
        for lines, blocked, warned in (
            (500, False, False),
            (501, False, True),
            (800, False, True),
            (801, True, False),
        ):
            with self.subTest(lines=lines):
                path = self.source("capture/source.rs", lines)
                self.assertEqual(rules.line_count(path), lines)
                errors, warnings = self.findings(path)
                self.assertEqual(bool(errors), blocked)
                self.assertEqual(bool(warnings), warned)

    def test_build_script_uses_ordinary_physical_size_and_exact_exception_policy(self):
        path = self.root / "build.rs"
        path.write_text("// authored build context\n" * 801)
        self.assertIn(path, rules.rust_source_files())
        self.assertTrue(self.findings(path)[0])
        exception = {
            "path": "build.rs",
            "maxLines": 850,
            "reason": "One bounded resource declaration",
        }
        errors, warnings = self.findings(path, [exception])
        self.assertFalse(errors)
        self.assertTrue(warnings)
        path.write_text("#![allow(clippy::too_many_lines)]\n")
        errors = []
        rules.check_function_size_exceptions(errors)
        self.assertTrue(errors)

    def test_entrypoint_and_facade_limits_are_still_blocking(self):
        for name, limit in (("main.rs", 80), ("capture/mod.rs", 120)):
            self.assertEqual(self.findings(self.source(name, limit)), ([], []))
            errors, warnings = self.findings(self.source(name, limit + 1))
            self.assertTrue(errors)
            self.assertFalse(warnings)

    def test_test_generated_and_table_sources_have_no_blanket_exemption(self):
        for name in (
            "capture/tests.rs",
            "capture/tests/scenario.rs",
            "tray/generated.rs",
            "tray/declarations.rs",
            "install/windows/adapter.rs",
        ):
            self.assertTrue(self.findings(self.source(name, 801))[0], name)

    def test_exact_exception_warns_but_cannot_hide_overflow_or_sibling(self):
        path = self.source("capture/tests.rs", 801)
        entry = {
            "path": "src/capture/tests.rs",
            "maxLines": 900,
            "reason": "One scenario with shared ports",
        }
        errors, warnings = self.findings(path, [entry])
        self.assertFalse(errors)
        self.assertTrue(warnings)
        self.assertTrue(self.findings(self.source("capture/tests.rs", 901), [entry])[0])
        sibling = self.source("capture/other.rs", 801)
        errors, warnings = [], []
        rules.file_size_findings([path, sibling], [entry], errors, warnings)
        self.assertTrue(any("other.rs" in error for error in errors))

    def test_stale_missing_duplicate_wildcard_and_unjustified_exceptions_fail(self):
        path = self.source("capture/tests.rs", 801)
        valid = {
            "path": "src/capture/tests.rs",
            "maxLines": 900,
            "reason": "Existing scenario",
        }
        for change in (
            {"reason": " "},
            {"path": "src/capture/*.rs"},
            {"path": "src/capture/missing.rs"},
            {"maxLines": 800},
            {"path": "src/../src/capture/tests.rs"},
        ):
            self.assertTrue(self.findings(path, [valid | change])[0], change)
        self.assertTrue(self.findings(path, [valid, valid])[0])
        self.assertTrue(self.findings(self.source("capture/tests.rs", 800), [valid])[0])

    def test_directory_counts_warn_at_21_and_61_without_changing_errors(self):
        owner = self.src / "capture"
        for index in range(20):
            self.source(f"capture/direct{index}.rs", 1)
        warnings = []
        rules.module_size_warnings(owner, warnings)
        self.assertFalse(warnings)
        self.source("capture/mod.rs", 1)
        rules.module_size_warnings(owner, warnings)
        self.assertEqual(len(warnings), 1)
        for index in range(39):
            self.source(f"capture/tests/nested{index}.rs", 1)
        warnings = []
        rules.module_size_warnings(owner, warnings)
        self.assertEqual(len(warnings), 1)
        self.source("capture/tests/sixtyoneth.rs", 1)
        warnings = []
        rules.module_size_warnings(owner, warnings)
        self.assertEqual(len(warnings), 2)
        errors = []
        rules.check_directory_shape(errors, [])
        self.assertFalse(errors)

    def test_function_exception_requires_exact_function_and_reason(self):
        path = self.source("capture/source.rs", 0)
        for attribute in (
            '#![expect(clippy::too_many_lines, reason = "test")]',
            '#[allow(clippy::too_many_lines, reason = "test")]',
            "#[expect(clippy::too_many_lines)]",
            '#[expect(clippy::too_many_lines, reason = " ")]',
            '#[cfg_attr(windows, allow(clippy::too_many_lines, reason = "broad"))]',
        ):
            path.write_text(attribute + "\nfn example() {}\n")
            errors = []
            rules.check_function_size_exceptions(errors)
            self.assertTrue(errors, attribute)
        attr = '#[expect(clippy::too_many_lines, reason = "Single owner transaction")]'
        path.write_text(attr + "\nmod broad;\n")
        errors = []
        rules.check_function_size_exceptions(errors)
        self.assertTrue(errors)
        path.write_text(attr + "\npub(crate) fn example() {}\n")
        errors = []
        rules.check_function_size_exceptions(errors)
        self.assertFalse(errors)

    def test_warning_survives_parallel_capture_without_failing_under_warnings_as_errors(
        self,
    ):
        from tooling.qa.runtime import parallel
        import sys

        record, log = self.root / "record.json", self.root / "run.log"
        storage.atomic_write_json(record, {"steps": [], "log": {"path": "run.log"}})
        warning = "WARN code-size: src/capture/source.rs has 501 physical lines"
        output = io.StringIO()
        with (
            mock.patch.object(parallel, "ROOT", self.root),
            mock.patch.object(run, "ROOT", self.root),
            mock.patch.dict(os.environ, {"PYTHONWARNINGS": "error", "RUSTFLAGS": "-D warnings"}),
            contextlib.redirect_stderr(output),
        ):
            code = parallel.run_group(
                [("boundary", [sys.executable, "-c", f"print({warning!r})"])],
                record,
                log,
                1,
            )
        self.assertEqual(code, 0)
        self.assertIn(warning, output.getvalue())
        self.assertIn(warning, log.read_text())
        self.assertEqual(storage.read_json(record)["steps"][0]["exitCode"], 0)

    def test_checkpoint_cannot_omit_or_weaken_windows_clippy(self):
        from tooling.qa.rules import verify_test_policy

        checkpoint = verify_test_policy.CHECKPOINT_WRAPPER.read_text()
        command = "cargo clippy --target x86_64-pc-windows-msvc --all-targets --all-features -- -D warnings"
        self.assertIn(command, checkpoint)
        for changed in (
            "true",
            command.replace("--all-targets", "--bin sniptale-native-agent"),
            command.replace("-D warnings", "-W warnings"),
        ):
            with self.subTest(changed=changed):
                wrapper = self.root / "checkpoint.sh"
                wrapper.write_text(checkpoint.replace(command, changed))
                output = io.StringIO()
                with (
                    mock.patch.object(verify_test_policy, "CHECKPOINT_WRAPPER", wrapper),
                    contextlib.redirect_stdout(output),
                    self.assertRaises(SystemExit),
                ):
                    verify_test_policy.check_release_wrapper_contract()
                self.assertIn(
                    "checkpoint must run native Clippy on Windows-only functions",
                    output.getvalue(),
                )


class NativeClippySizeTests(unittest.TestCase):
    def test_pinned_clippy_counting_and_expectation_staleness(self):
        # Execute the pinned compiler, never reproduce Clippy's line parser here.
        root = Path(__file__).resolve().parents[3]
        with tempfile.TemporaryDirectory(prefix="sniptale-clippy-size-") as directory:
            source = Path(directory) / "probe.rs"
            body = "    std::hint::black_box(1_u8);\n"
            cases = (
                (100, "", "", True, None),
                (101, "", "", False, "too_many_lines"),
                (100, "\n// comment\n/* multi\nline comment */\n", "", True, None),
                (
                    101,
                    "",
                    '#[expect(clippy::too_many_lines, reason = "One transaction")]\n',
                    True,
                    None,
                ),
                (
                    100,
                    "",
                    '#[expect(clippy::too_many_lines, reason = "One transaction")]\n',
                    False,
                    "unfulfilled_lint_expectations",
                ),
            )
            for target in (None, "x86_64-pc-windows-msvc"):
                for lines, padding, attribute, accepted, diagnostic in cases:
                    with self.subTest(
                        target=target,
                        lines=lines,
                        padding=bool(padding),
                        attribute=bool(attribute),
                    ):
                        source.write_text(
                            attribute + "pub fn probe(\n) {\n" + padding + (body + padding) * lines + "}\n"
                        )
                        env = os.environ | {"CLIPPY_CONF_DIR": str(root)}
                        result = subprocess.run(
                            [
                                "clippy-driver",
                                *(["--target", target] if target else []),
                                "--edition=2024",
                                "--crate-type=lib",
                                "--emit=metadata",
                                "-Dwarnings",
                                "-Dclippy::too_many_lines",
                                str(source),
                                "-o",
                                str(Path(directory) / "probe.rmeta"),
                            ],
                            cwd=root,
                            env=env,
                            capture_output=True,
                            text=True,
                            check=False,
                            timeout=30,
                        )
                        self.assertEqual(result.returncode == 0, accepted, result.stderr)
                        if diagnostic:
                            self.assertIn(diagnostic, result.stderr)
