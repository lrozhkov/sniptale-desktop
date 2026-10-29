from __future__ import annotations

import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tooling/release/windows"))
import alpha_artifacts as artifacts
import alpha_publish as publish
import alpha_acceptance as acceptance


class AlphaArtifactTests(unittest.TestCase):
    commit = "a" * 40
    tag = "v0.1.0-alpha.1"

    def bundle(self, directory: Path) -> None:
        qa = directory / "qa"
        qa.mkdir()
        for name in artifacts.PROOF_FILES - {"qa-proof.json"}:
            (qa / name).write_text("{}")
        artifacts.write_json(qa / "qa-proof.json", {
            "schemaVersion": 1, "kind": "sniptale-alpha-qa", "commit": self.commit,
            "tree": "b" * 40, "gates": {name: {"runId": "local-gate", "recordSha256": "f" * 64} for name in artifacts.GATES}, "files": artifacts.files(qa),
            "runId": "local", "runAttempt": "local",
        })
        for source, target in [("LICENSE", "LICENSE.txt"), ("NOTICE", "NOTICE.txt"),
                               ("THIRD_PARTY_NOTICES.md", "THIRD_PARTY_NOTICES.md"),
                               ("THIRD_PARTY_DEPENDENCIES.json", "THIRD_PARTY_DEPENDENCIES.json")]:
            shutil.copyfile(ROOT / source, directory / target)
        shutil.copytree(ROOT / "LICENSES", directory / "LICENSES")
        (directory / "SniptaleNativeAgent-dev-0.1.0.msi").write_bytes(b"msi")
        (directory / "sniptale-native-agent.exe").write_bytes(b"exe")
        (directory / "build-environment.json").write_text("{}")
        artifacts.write_json(directory / "native-host.json", {
            "name": "com.sniptale.dev.native_host", "allowed_origins": [artifacts.ORIGIN], "path": "sniptale-native-agent.exe",
        })
        with mock.patch.object(artifacts, "assert_source"), mock.patch.object(artifacts, "git", return_value="b" * 40), mock.patch.dict(artifacts.os.environ, {"GITHUB_RUN_ID": "local", "GITHUB_RUN_ATTEMPT": "local"}):
            artifacts.seal(directory, self.commit, self.tag)

    def test_bundle_accepts_exact_inventory_and_rejects_mutations(self):
        for mutation in ("none", "extra", "missing", "corrupt", "symlink", "identity"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as name:
                directory = Path(name)
                self.bundle(directory)
                if mutation == "extra":
                    (directory / "unexpected.exe").write_text("data")
                elif mutation == "missing":
                    (directory / "qa/sbom.spdx.json").unlink()
                elif mutation == "corrupt":
                    (directory / "sniptale-native-agent.exe").write_text("changed")
                elif mutation == "symlink":
                    (directory / "link").symlink_to(directory / "LICENSE.txt")
                elif mutation == "identity":
                    value = artifacts.read_json(directory / "provenance.json")
                    value["devLocalTrust"] = True
                    artifacts.write_json(directory / "provenance.json", value)
                if mutation == "none":
                    artifacts.verify_bundle(directory, self.commit, self.tag)
                else:
                    with self.assertRaises(ValueError):
                        artifacts.verify_bundle(directory, self.commit, self.tag)

    def test_publication_rejects_wrong_run_attempt_repository_and_missing_cases(self):
        run = {"id": 12, "run_attempt": 2, "head_sha": self.commit, "head_branch": "main",
               "event": "workflow_dispatch", "conclusion": "success", "status": "completed",
               "path": ".github/workflows/alpha.yml", "repository": {"full_name": publish.REPO},
               "head_repository": {"full_name": publish.REPO}}
        artifact = {"id": 23, "name": f"alpha-{self.commit}-12-2", "expired": False, "digest": "sha256:" + "c" * 64}
        receipt = {"schemaVersion": 1, "kind": "sniptale-alpha-windows-acceptance", "commit": self.commit,
                   "tag": self.tag, "runId": "12", "runAttempt": "2", "artifactId": 23,
                   "artifactDigest": artifact["digest"], "result": "pass", "verified": True,
                   "cleanTeardown": True, "devLocalTrust": False,
                   "tests": {name: "pass" for name in sorted(publish.RELEASE_CASES)},
                   "checksumsSha256": "d" * 64, "receiptSha256": "e" * 64}
        publish.admit(run, artifact, receipt, self.commit, self.tag)
        for target, field, value in [(run, "event", "pull_request"), (run, "run_attempt", 3),
                                      (run, "head_repository", {"full_name": "other/fork"}),
                                      (artifact, "expired", True), (receipt, "tests", {"capture": "pass"}),
                                      (receipt, "devLocalTrust", True)]:
            with self.subTest(field=field), mock.patch.dict(target, {field: value}):
                with self.assertRaises(ValueError):
                    publish.admit(run, artifact, receipt, self.commit, self.tag)

    def test_tag_rejects_stable_path_and_shell_inputs(self):
        for value in ("v1.0.0", "../v0.1.0-alpha.1", "v0.1.0-alpha.0", "v0.1.0-alpha.1;id"):
            with self.assertRaises(ValueError):
                artifacts.version_tag(value)

    def test_diagnostic_dirty_duplicate_or_forced_receipt_cannot_admit_release(self):
        import copy
        import hashlib
        receipt = {"result": "pass", "run_mode": "alpha-release-acceptance", "commit_sha": self.commit,
                   "dirty_state_sha256": hashlib.sha256(b"").hexdigest(), "lanes": ["windows-11"],
                   "teardown": {"windows-11": {"post_forced": False}},
                   "test_matrix": [{"lane": "windows-11", "tests": [{"name": name, "status": "pass"} for name in sorted(publish.RELEASE_CASES)]}]}
        acceptance.verified_tests(receipt, self.commit)
        for mutation in ("dirty", "mode", "duplicate", "forced", "missing", "lane"):
            value = copy.deepcopy(receipt)
            if mutation == "dirty": value["dirty_state_sha256"] = "f" * 64
            elif mutation == "mode": value["run_mode"] = "focused-test"
            elif mutation == "duplicate": value["test_matrix"][0]["tests"].append(value["test_matrix"][0]["tests"][0])
            elif mutation == "forced": value["teardown"]["windows-11"]["post_forced"] = True
            elif mutation == "missing": value["test_matrix"][0]["tests"].pop()
            else: value["lanes"] = ["linux-x11"]
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                acceptance.verified_tests(value, self.commit)

    def test_tag_creation_explicitly_uses_post_with_structured_body(self):
        with mock.patch.object(publish.subprocess, "check_output", return_value="{}") as request:
            publish.api("git/refs", {"ref": "refs/tags/v0.1.0-alpha.1", "sha": self.commit})
        command = request.call_args.args[0]
        self.assertEqual(command[-4:], ["--method", "POST", "--input", "-"])
        self.assertEqual(json.loads(request.call_args.kwargs["input"])["sha"], self.commit)

    def test_cold_bootstrap_installs_cli_and_rejects_missing_executable(self):
        versions = json.loads((ROOT / "tooling/configs/qa/tool-versions.json").read_text())
        for missing in (False, True):
            with self.subTest(missing=missing), tempfile.TemporaryDirectory() as name:
                root = Path(name)
                installers = root / "tooling/qa/installers"
                runtime = root / "tooling/qa/runtime"
                configs = root / "tooling/configs/qa"
                binaries = root / "bin"
                for directory in (installers, runtime, configs, binaries):
                    directory.mkdir(parents=True)
                shutil.copyfile(ROOT / "tooling/qa/installers/install-release-tools.sh", installers / "install-release-tools.sh")
                (runtime / "lib.sh").write_text("ensure_rust_env() { :; }\n")
                (configs / "tool-versions.json").write_text(json.dumps(versions))
                for executable, body in {"rustup": "exit 0", "gitleaks-installer": "exit 0"}.items():
                    target = installers / "install-gitleaks.sh" if executable == "gitleaks-installer" else binaries / executable
                    target.write_text("#!/usr/bin/env bash\n" + body + "\n")
                    target.chmod(0o700)
                for tool, version in versions["cargoSubcommands"].items():
                    if tool != "cargo-about":
                        target = binaries / tool
                        guard = '[[ "${1:-}" == llvm-cov && "${2:-}" == --version ]] || exit 2\n' if tool == "cargo-llvm-cov" else ""
                        target.write_text(f"#!/usr/bin/env bash\n{guard}echo '{tool} {version}'\n")
                        target.chmod(0o700)
                cargo = binaries / "cargo"
                cargo.write_text('''#!/usr/bin/env python3
import os, pathlib, subprocess, sys
args = sys.argv[1:]
if args[0] != 'install':
    sys.exit(subprocess.run([str(pathlib.Path(os.environ['FAKE_BIN']) / ('cargo-' + args[0])), *args]).returncode)
if '--features' in args and args[args.index('--features')+1] == 'cli' and os.environ['FAKE_MISSING'] != '1':
    target = pathlib.Path(os.environ['FAKE_BIN']) / 'cargo-about'
    target.write_text('#!/usr/bin/env bash\\necho "cargo-about 0.9.1"\\n')
    target.chmod(0o700)
''')
                cargo.chmod(0o700)
                environment = {**os.environ, "PATH": str(binaries) + os.pathsep + "/usr/bin:/bin",
                               "FAKE_BIN": str(binaries), "FAKE_MISSING": "1" if missing else "0"}
                result = subprocess.run(["bash", str(installers / "install-release-tools.sh")], env=environment, capture_output=True, text=True)
                self.assertEqual(result.returncode, 1 if missing else 0, result.stderr)


if __name__ == "__main__":
    unittest.main()
