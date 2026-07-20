from __future__ import annotations

import importlib.util
import json
import os
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest import mock


ROOT = Path(__file__).resolve().parents[3]


class RegistryTests(unittest.TestCase):
    @staticmethod
    def _executable(path: Path, body: str) -> Path:
        path.write_text("#!/usr/bin/env bash\nset -euo pipefail\n" + body, encoding="utf-8")
        path.chmod(0o700)
        return path

    def _fake_closeout_environment(
        self,
        directory: Path,
        *,
        checkpoint_exit: int = 0,
        high_risk: bool = False,
        dependency: bool = False,
        drift: bool = False,
        staged_status: str = "",
    ) -> tuple[dict[str, str], Path]:
        trace = directory / "trace.log"
        marker = directory / "drift.marker"
        git = self._executable(directory / "git", r'''
case "${1:-}" in
  rev-parse)
    if [[ "$*" == *"--is-inside-work-tree"* ]]; then
      printf 'true\n'
    elif [[ -f "$DRIFT_MARKER" ]]; then
      printf 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n'
    else
      printf 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n'
    fi
    ;;
  diff)
    if [[ "$*" == *"--cached --name-status"* && -n "${FAKE_STAGED_STATUS:-}" ]]; then
      printf '%b' "$FAKE_STAGED_STATUS"
    fi
    ;;
  ls-files)
    ;;
  status)
    if [[ "${FAKE_HIGH_RISK:-0}" == 1 && "$*" == *"src/ipc"* ]]; then
      printf ' M src/ipc/fake.rs\n'
    elif [[ "${FAKE_DEPENDENCY:-0}" == 1 && "$*" == *"Cargo.toml"* ]]; then
      printf ' M Cargo.toml\n'
    fi
    ;;
  add|commit)
    printf 'git-%s %s\n' "$1" "$*" >> "$TRACE_FILE"
    printf '%s\n' "$*"
    ;;
esac
''')
        checkpoint = self._executable(directory / "checkpoint", f'''
printf 'checkpoint\n' >> "$TRACE_FILE"
if [[ "${{FAKE_DRIFT:-0}}" == 1 ]]; then touch "$DRIFT_MARKER"; fi
exit {checkpoint_exit}
''')
        coverage = self._executable(
            directory / "coverage", 'printf \'coverage\\n\' >> "$TRACE_FILE"\n'
        )
        supply = self._executable(
            directory / "supply-chain", 'printf \'supply-chain\\n\' >> "$TRACE_FILE"\n'
        )
        build = self._executable(
            directory / "build", 'printf \'build\\n\' >> "$TRACE_FILE"\n'
        )
        environment = os.environ.copy()
        environment.update({
            "PATH": f"{directory}:{environment['PATH']}",
            "TRACE_FILE": str(trace),
            "DRIFT_MARKER": str(marker),
            "FAKE_HIGH_RISK": "1" if high_risk else "0",
            "FAKE_DEPENDENCY": "1" if dependency else "0",
            "FAKE_DRIFT": "1" if drift else "0",
            "FAKE_STAGED_STATUS": staged_status,
            "SNIPTALE_QA_CHECKPOINT_WRAPPER": str(checkpoint),
            "SNIPTALE_QA_COVERAGE_WRAPPER": str(coverage),
            "SNIPTALE_QA_SUPPLY_CHAIN_WRAPPER": str(supply),
            "SNIPTALE_QA_BUILD_WRAPPER": str(build),
        })
        self.assertTrue(git.is_file())
        return environment, trace

    def test_registry_and_topology_guard_are_green(self) -> None:
        path = ROOT / "tooling/qa/rules/verify_tooling.py"
        spec = importlib.util.spec_from_file_location("verify_tooling_test", path)
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        errors: list[str] = []
        module.validate_registry(errors)
        module.validate_wrapper_topology(errors)
        module.validate_zero_baselines(errors)
        self.assertEqual(errors, [])

    def test_clean_break_identity_detector_rejects_every_retired_root_and_case(self) -> None:
        path = ROOT / "tooling/qa/rules/verify_tooling.py"
        spec = importlib.util.spec_from_file_location("verify_tooling_identity_test", path)
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        roots = module.retired_product_roots()
        self.assertEqual(len(roots), 4)
        for root in roots:
            self.assertEqual(module.retired_identity_matches(f"prefix-{root}-suffix"), (root,))
            self.assertEqual(module.retired_identity_matches(root.upper()), (root,))
        self.assertEqual(module.retired_identity_matches("SniptaleNativeAgent"), ())

    def test_publication_controls_are_green(self) -> None:
        path = ROOT / "tooling/qa/rules/verify_tooling.py"
        spec = importlib.util.spec_from_file_location("verify_tooling_publication_test", path)
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        errors: list[str] = []
        module.validate_hosted_qa_policy(errors)
        module.validate_git_attributes(errors)
        module.validate_secret_audit(errors)
        module.validate_retired_smoke_queue(errors)
        self.assertEqual(errors, [])

    def test_hosted_qa_policy_rejects_github_actions_workflows(self) -> None:
        path = ROOT / "tooling/qa/rules/verify_tooling.py"
        spec = importlib.util.spec_from_file_location("verify_tooling_hosted_qa_test", path)
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            workflow = root / ".github/workflows/ci.yml"
            workflow.parent.mkdir(parents=True)
            workflow.write_text("name: hosted QA\n", encoding="utf-8")
            errors: list[str] = []
            with mock.patch.object(module, "ROOT", root):
                module.validate_hosted_qa_policy(errors)
            self.assertEqual(
                errors,
                [
                    "hosted GitHub Actions workflows are disabled; run canonical QA locally: "
                    ".github/workflows/ci.yml"
                ],
            )

    def test_temporary_debt_schema_requires_expiry_and_removal_condition(self) -> None:
        path = ROOT / "tooling/qa/rules/verify_tooling.py"
        spec = importlib.util.spec_from_file_location("verify_tooling_debt_test", path)
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as directory:
            registry = Path(directory) / "exceptions.json"
            registry.write_text(json.dumps({
                "schemaVersion": 1,
                "technicalDebt": [],
                "exceptions": [{
                    "id": "debt.example",
                    "ruleId": "test.example",
                    "classification": "temporary-debt",
                    "owner": "qa-owner",
                    "exactScope": {"exactPaths": ["AGENTS.md"]},
                    "rationale": "schema test",
                    "risk": "schema test risk",
                    "proofReferences": ["AGENTS.md"],
                }],
            }), encoding="utf-8")
            errors: list[str] = []
            with mock.patch.object(module, "REGISTRY", registry):
                module.validate_registry(errors)
        self.assertTrue(any("reviewBy and removalCondition" in error for error in errors))
        self.assertTrue(any("forbidden in this wave" in error for error in errors))

    def test_retired_path_guard_ignores_policy_and_historical_evidence(self) -> None:
        path = ROOT / "tooling/qa/rules/verify_tooling.py"
        spec = importlib.util.spec_from_file_location("verify_tooling_retired_path_test", path)
        assert spec is not None and spec.loader is not None
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        errors: list[str] = []
        with mock.patch.object(
            module,
            "git",
            side_effect=[
                "",
                'tooling/configs/qa/docs.json:42:  "forbiddenFragments": ["'
                + "scripts"
                + '/qa/"]\n'
                + 'tooling/configs/qa/gitleaks-baseline.json:15:    "File": "'
                + "scripts"
                + '/e2e_lab/guest/manifest.json"\n',
            ],
        ):
            module.validate_retired_scripts(errors)
        self.assertEqual(errors, [])

    def test_closeout_orders_proof_before_staging_and_commit(self) -> None:
        text = (ROOT / "tooling/qa/wrappers/closeout.sh").read_text(encoding="utf-8")
        proof = text.index('qa_step "build"')
        failure_gate = text.index('qa_fail_if_any "closeout proof summary"')
        staging = text.index("git add -A")
        commit = text.index('qa_step "git commit"')
        self.assertLess(proof, failure_gate)
        self.assertLess(failure_gate, staging)
        self.assertLess(staging, commit)
        self.assertIn("fingerprint_worktree", text)

    def test_closeout_fake_commands_run_conditional_coverage_in_order(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            environment, trace = self._fake_closeout_environment(
                Path(directory), high_risk=True
            )
            completed = subprocess.run(
                ["tooling/qa/wrappers/closeout.sh", "--no-commit"],
                cwd=ROOT,
                env=environment,
                text=True,
                capture_output=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(
                trace.read_text(encoding="utf-8").splitlines(),
                ["checkpoint", "coverage", "checkpoint", "build"],
            )

    def test_closeout_fake_commands_run_conditional_supply_chain(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            environment, trace = self._fake_closeout_environment(
                Path(directory), dependency=True
            )
            completed = subprocess.run(
                ["tooling/qa/wrappers/closeout.sh", "--no-commit"],
                cwd=ROOT,
                env=environment,
                text=True,
                capture_output=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertEqual(
                trace.read_text(encoding="utf-8").splitlines(),
                ["checkpoint", "supply-chain", "checkpoint", "build"],
            )

    def test_closeout_propagates_failure_and_never_commits_after_error(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            environment, trace = self._fake_closeout_environment(
                Path(directory), checkpoint_exit=9
            )
            completed = subprocess.run(
                ["tooling/qa/wrappers/closeout.sh", "-m", "must not commit"],
                cwd=ROOT,
                env=environment,
                text=True,
                capture_output=True,
            )
            self.assertEqual(completed.returncode, 9, completed.stderr)
            self.assertNotIn("git-add", trace.read_text(encoding="utf-8"))
            self.assertNotIn("git-commit", trace.read_text(encoding="utf-8"))

    def test_closeout_rejects_diff_drift_after_proof(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            environment, _trace = self._fake_closeout_environment(
                Path(directory), drift=True
            )
            completed = subprocess.run(
                ["tooling/qa/wrappers/closeout.sh", "--no-commit"],
                cwd=ROOT,
                env=environment,
                text=True,
                capture_output=True,
            )
            self.assertEqual(completed.returncode, 1)
            self.assertIn("worktree changed", completed.stderr)

    def test_closeout_allows_legacy_tmp_deletion_but_rejects_tmp_addition(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            environment, trace = self._fake_closeout_environment(
                Path(directory), staged_status="D\\t.tmp/audit/legacy.json\\n"
            )
            completed = subprocess.run(
                ["tooling/qa/wrappers/closeout.sh", "-m", "delete legacy temp"],
                cwd=ROOT,
                env=environment,
                text=True,
                capture_output=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            self.assertIn("git-commit", trace.read_text(encoding="utf-8"))

        with tempfile.TemporaryDirectory() as directory:
            environment, trace = self._fake_closeout_environment(
                Path(directory), staged_status="A\\t.tmp/new-artifact.log\\n"
            )
            completed = subprocess.run(
                ["tooling/qa/wrappers/closeout.sh", "-m", "reject temp addition"],
                cwd=ROOT,
                env=environment,
                text=True,
                capture_output=True,
            )
            self.assertEqual(completed.returncode, 1, completed.stderr)
            self.assertIn("Refusing to add or modify", completed.stderr)
            self.assertNotIn("git-commit", trace.read_text(encoding="utf-8"))

    def test_closeout_redacts_commit_message_and_wrapper_resolves_root(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            temporary = Path(directory)
            environment, _trace = self._fake_closeout_environment(temporary)
            before = set((ROOT / ".tmp/qa-observability/runs").glob("**/*.json"))
            message = "private closeout title"
            completed = subprocess.run(
                [str(ROOT / "tooling/qa/wrappers/closeout.sh"), "-m", message],
                cwd=temporary,
                env=environment,
                text=True,
                capture_output=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            after = set((ROOT / ".tmp/qa-observability/runs").glob("**/*.json"))
            records = [
                path for path in after - before
                if json.loads(path.read_text(encoding="utf-8")).get("wrapperId") == "qa:closeout"
            ]
            self.assertEqual(len(records), 1)
            record = json.loads(records[0].read_text(encoding="utf-8"))
            log = ROOT / record["log"]["path"]
            self.assertNotIn(message, log.read_text(encoding="utf-8"))

            help_result = subprocess.run(
                [str(ROOT / "tooling/qa/wrappers/prune-target.sh"), "--help"],
                cwd=temporary,
                text=True,
                capture_output=True,
            )
            self.assertEqual(help_result.returncode, 0, help_result.stderr)
            self.assertIn("Usage: tooling/qa/wrappers/prune-target.sh", help_result.stdout)


if __name__ == "__main__":
    unittest.main()
