from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path
import unittest.mock as mock

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tooling/release/windows"))
import alpha_artifacts as artifacts  # noqa: E402 -- resolve the standalone release owner after its path is installed.
import alpha_publish as publish  # noqa: E402 -- resolve the standalone release owner after its path is installed.
import alpha_acceptance as acceptance  # noqa: E402 -- resolve the standalone release owner after its path is installed.


class AlphaArtifactTests(unittest.TestCase):
    commit = "a" * 40
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    tag = f"v{version}-alpha.1"

    @staticmethod
    def frontend(directory: Path) -> None:
        (directory / "assets").mkdir(parents=True)
        (directory / "index.html").write_text(
            '<script src="/assets/app-123.js"></script><link href="/assets/app-123.css">'
        )
        (directory / "assets/app-123.js").write_text("export {};")
        (directory / "assets/app-123.css").write_text("@font-face{src:url(/assets/font-123.woff2)}")
        (directory / "assets/font-123.woff2").write_bytes(b"font")

    def test_frontend_handoff_rejects_missing_extra_redirected_and_changed_assets(self):
        for mutation in ("none", "missing", "extra", "symlink", "changed"):
            with self.subTest(mutation=mutation), tempfile.TemporaryDirectory() as name:
                directory = Path(name)
                self.bundle(directory)
                frontend = directory / "qa/frontend"
                if mutation == "missing":
                    (frontend / "assets/app-123.js").unlink()
                elif mutation == "extra":
                    (frontend / "assets/extra.js").write_text("extra")
                elif mutation == "symlink":
                    (frontend / "assets/app-123.js").unlink()
                    (frontend / "assets/app-123.js").symlink_to(frontend / "index.html")
                elif mutation == "changed":
                    (frontend / "assets/app-123.css").write_text("changed")
                if mutation == "none":
                    artifacts.verify_proof(directory / "qa", self.commit)
                else:
                    with self.assertRaises(ValueError):
                        artifacts.verify_proof(directory / "qa", self.commit)

    def test_frontend_inventory_rejects_incomplete_reference_even_with_fresh_hashes(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            self.frontend(directory / "frontend")
            (directory / "frontend/assets/font-123.woff2").unlink()
            with self.assertRaises(ValueError):
                artifacts.frontend_files(directory / "frontend")

    def test_windows_packager_copies_verified_frontend_before_cargo(self):
        script = (ROOT / "tooling/release/windows/alpha-package.ps1").read_text()
        verified = script.index("alpha_artifacts.py verify-proof")
        copied = script.index('Copy-Item -LiteralPath (Join-Path $QaProof "frontend")')
        self.assertLess(verified, copied)
        self.assertLess(copied, script.index("& cargo build"))

    def test_release_handoff_rejects_review_only_analysis(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            records = root / ".tmp/qa-observability/runs/day"
            records.mkdir(parents=True)
            for index, owner in enumerate(sorted(artifacts.GATES)):
                record = {
                    "wrapperId": owner,
                    "mode": "review-existing-results" if owner == "qa:static-analysis" else "default",
                    "status": "all-passed",
                    "exitCode": 0,
                    "runId": f"run-{index}",
                    "repository": {"revision": self.commit, "targetFiles": []},
                }
                (records / f"record-{index}.json").write_text(json.dumps(record))
            for name in [
                ".tmp/release/sbom.spdx.json",
                ".tmp/release/supply-chain/cargo-audit.json",
                ".tmp/release/supply-chain/cargo-deny.txt",
            ]:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("{}\n")
            self.frontend(root / "ui/dist")
            with (
                mock.patch.object(artifacts, "ROOT", root),
                mock.patch.object(artifacts, "git", return_value=self.commit),
                mock.patch.object(artifacts, "assert_source"),
            ):
                with self.assertRaisesRegex(ValueError, "missing clean-commit canonical release gates"):
                    artifacts.make_proof(root / "rejected")
                for path in records.glob("*.json"):
                    record = json.loads(path.read_text())
                    record["mode"] = "default"
                    path.write_text(json.dumps(record))
                artifacts.make_proof(root / "accepted")
                proof = artifacts.verify_proof(root / "accepted", self.commit)
                self.assertIn("qa:static-analysis", proof["gates"])

    def bundle(self, directory: Path) -> None:
        qa = directory / "qa"
        qa.mkdir()
        for name in artifacts.PROOF_FILES - {"qa-proof.json"}:
            (qa / name).write_text("{}")
        self.frontend(qa / "frontend")
        artifacts.write_json(
            qa / "qa-proof.json",
            {
                "schemaVersion": 1,
                "kind": "sniptale-alpha-qa",
                "commit": self.commit,
                "tree": "b" * 40,
                "gates": {name: {"runId": "local-gate", "recordSha256": "f" * 64} for name in artifacts.GATES},
                "files": artifacts.files(qa),
                "runId": "local",
                "runAttempt": "local",
            },
        )
        for source, target in [
            ("LICENSE", "LICENSE.txt"),
            ("NOTICE", "NOTICE.txt"),
            ("THIRD_PARTY_NOTICES.md", "THIRD_PARTY_NOTICES.md"),
            ("THIRD_PARTY_DEPENDENCIES.json", "THIRD_PARTY_DEPENDENCIES.json"),
        ]:
            shutil.copyfile(ROOT / source, directory / target)
        shutil.copytree(ROOT / "LICENSES", directory / "LICENSES")
        (directory / f"SniptaleNativeAgent-dev-{self.version}.msi").write_bytes(b"msi")
        (directory / "sniptale-native-agent.exe").write_bytes(b"exe")
        (directory / "build-environment.json").write_text("{}")
        artifacts.write_json(
            directory / "native-host.json",
            {
                "name": "com.sniptale.dev.native_host",
                "allowed_origins": [artifacts.ORIGIN],
                "path": "sniptale-native-agent.exe",
            },
        )
        with (
            mock.patch.object(artifacts, "assert_source"),
            mock.patch.object(artifacts, "git", return_value="b" * 40),
            mock.patch.dict(artifacts.os.environ, {"GITHUB_RUN_ID": "local", "GITHUB_RUN_ATTEMPT": "local"}),
        ):
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
        run = {
            "id": 12,
            "run_attempt": 2,
            "head_sha": self.commit,
            "head_branch": "main",
            "event": "workflow_dispatch",
            "conclusion": "success",
            "status": "completed",
            "path": ".github/workflows/alpha.yml",
            "repository": {"full_name": publish.REPO},
            "head_repository": {"full_name": publish.REPO},
        }
        artifact = {"id": 23, "name": f"alpha-{self.commit}-12-2", "expired": False, "digest": "sha256:" + "c" * 64}
        receipt = {
            "schemaVersion": 1,
            "kind": "sniptale-alpha-windows-acceptance",
            "commit": self.commit,
            "tag": self.tag,
            "runId": "12",
            "runAttempt": "2",
            "artifactId": 23,
            "artifactDigest": artifact["digest"],
            "result": "pass",
            "verified": True,
            "cleanTeardown": True,
            "devLocalTrust": False,
            "tests": {name: "pass" for name in sorted(publish.RELEASE_CASES)},
            "checksumsSha256": "d" * 64,
            "receiptSha256": "e" * 64,
        }
        publish.admit(run, artifact, receipt, self.commit, self.tag)
        for target, field, value in [
            (run, "event", "pull_request"),
            (run, "run_attempt", 3),
            (run, "head_repository", {"full_name": "other/fork"}),
            (artifact, "expired", True),
            (receipt, "tests", {"capture": "pass"}),
            (receipt, "devLocalTrust", True),
        ]:
            with (
                self.subTest(field=field),
                mock.patch.dict(target, {field: value}),
                self.assertRaises(ValueError),
            ):
                publish.admit(run, artifact, receipt, self.commit, self.tag)

    def test_tag_rejects_stable_path_and_shell_inputs(self):
        for value in ("v1.0.0", "../v0.1.0-alpha.1", "v0.1.0-alpha.0", "v0.1.0-alpha.1;id"):
            with self.assertRaises(ValueError):
                artifacts.version_tag(value)

    def test_diagnostic_dirty_duplicate_or_forced_receipt_cannot_admit_release(self):
        import copy
        import hashlib

        receipt = {
            "result": "pass",
            "run_mode": "alpha-release-acceptance",
            "commit_sha": self.commit,
            "dirty_state_sha256": hashlib.sha256(b"").hexdigest(),
            "lanes": ["windows-11"],
            "teardown": {"windows-11": {"post_forced": False}},
            "test_matrix": [
                {
                    "lane": "windows-11",
                    "tests": [{"name": name, "status": "pass"} for name in sorted(publish.RELEASE_CASES)],
                }
            ],
        }
        acceptance.verified_tests(receipt, self.commit)
        for mutation in ("dirty", "mode", "duplicate", "forced", "missing", "lane"):
            value = copy.deepcopy(receipt)
            if mutation == "dirty":
                value["dirty_state_sha256"] = "f" * 64
            elif mutation == "mode":
                value["run_mode"] = "focused-test"
            elif mutation == "duplicate":
                value["test_matrix"][0]["tests"].append(value["test_matrix"][0]["tests"][0])
            elif mutation == "forced":
                value["teardown"]["windows-11"]["post_forced"] = True
            elif mutation == "missing":
                value["test_matrix"][0]["tests"].pop()
            else:
                value["lanes"] = ["linux-x11"]
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                acceptance.verified_tests(value, self.commit)

        generated = copy.deepcopy(receipt)
        generated["dirty_state_sha256"] = "b" * 64
        acceptance.verified_tests(generated, self.commit, source_fingerprint="b" * 64)
        with self.assertRaises(ValueError):
            acceptance.verified_tests(generated, self.commit, source_fingerprint="c" * 64)

    def test_generated_frontend_requires_clean_source_and_verified_candidate_bytes(self):
        import hashlib

        with tempfile.TemporaryDirectory() as name:
            root = Path(name) / "source"
            root.mkdir()
            (root / "ui/src").mkdir(parents=True)
            (root / "ui/src/app.ts").write_text("export {};\n")
            (root / "tauri.conf.json").write_text("{}\n")
            (root / ".gitignore").write_text("ui/dist/\n")
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            subprocess.run(["git", "-C", str(root), "add", "."], check=True)
            subprocess.run(
                [
                    "git",
                    "-C",
                    str(root),
                    "-c",
                    "user.name=Unit",
                    "-c",
                    "user.email=unit@example.invalid",
                    "commit",
                    "-qm",
                    "fixture",
                ],
                check=True,
            )
            commit = subprocess.check_output(["git", "-C", str(root), "rev-parse", "HEAD"], text=True).strip()
            bundle = Path(name) / "bundle"
            self.frontend(root / "ui/dist")
            self.frontend(bundle / "qa/frontend")
            with mock.patch.object(acceptance, "ROOT", root), mock.patch.object(artifacts, "ROOT", root):
                fingerprint = acceptance.candidate_source_fingerprint(bundle, commit)
                self.assertNotEqual(fingerprint, hashlib.sha256(b"").hexdigest())
                (root / "ui/src/app.ts").write_text("unreviewed source\n")
                with self.assertRaisesRegex(ValueError, "clean source checkout"):
                    acceptance.candidate_source_fingerprint(bundle, commit)
                (root / "ui/src/app.ts").write_text("export {};\n")
                untracked = root / "ui/src/extra.ts"
                untracked.write_text("unreviewed source\n")
                with self.assertRaisesRegex(ValueError, "clean source checkout"):
                    acceptance.candidate_source_fingerprint(bundle, commit)
                untracked.unlink()
                (root / "ui/dist/assets/app-123.css").write_text(
                    "@font-face{src:url(/assets/font-123.woff2)}body{color:red}"
                )
                with self.assertRaisesRegex(ValueError, "frontend differs"):
                    acceptance.candidate_source_fingerprint(bundle, commit)
                shutil.copyfile(bundle / "qa/frontend/assets/app-123.css", root / "ui/dist/assets/app-123.css")
                self.assertEqual(acceptance.candidate_source_fingerprint(bundle, commit), fingerprint)
                with self.assertRaisesRegex(ValueError, "commit mismatch"):
                    acceptance.candidate_source_fingerprint(bundle, "f" * 40)

    def test_publication_cannot_use_an_environment_audit_for_another_commit(self):
        environment = {
            "GITHUB_REF": "refs/heads/main",
            "GITHUB_SHA": "a" * 40,
            "ALPHA_COMMIT": "b" * 40,
            "ALPHA_RUN_ID": "1",
            "ALPHA_ARTIFACT_ID": "2",
        }
        with (
            mock.patch.dict(os.environ, environment, clear=True),
            mock.patch.object(publish, "api") as api,
            self.assertRaisesRegex(ValueError, "environment was audited"),
        ):
            publish.main()
        api.assert_not_called()

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
                shutil.copyfile(
                    ROOT / "tooling/qa/installers/install-release-tools.sh", installers / "install-release-tools.sh"
                )
                (runtime / "lib.sh").write_text("ensure_rust_env() { :; }\n")
                (configs / "tool-versions.json").write_text(json.dumps(versions))
                shutil.copyfile(ROOT / "tooling/configs/qa/toolchain-lock.json", configs / "toolchain-lock.json")
                for module in (
                    "toolchain.py",
                    "browser_toolchain.py",
                    "script_toolchain.py",
                    "python_toolchain.py",
                    "checking_dependency_ages.py",
                    "tool_dependencies.py",
                    "repository.py",
                    "advisories.py",
                ):
                    shutil.copyfile(ROOT / "tooling/qa/runtime" / module, runtime / module)
                for relative in (
                    "script-python-requirements.txt",
                    "script-python-build-requirements.txt",
                    "script-node/package-lock.json",
                ):
                    target = configs / relative
                    target.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copyfile(ROOT / "tooling/configs/qa" / relative, target)
                (root / "ui").mkdir()
                shutil.copyfile(ROOT / "ui/package-lock.json", root / "ui/package-lock.json")
                shutil.copyfile(ROOT / "rust-toolchain.toml", root / "rust-toolchain.toml")
                shutil.copyfile(ROOT / "Cargo.lock", root / "Cargo.lock")
                shutil.copyfile(ROOT / "Cargo.toml", root / "Cargo.toml")
                for executable, body in {"rustup": "exit 0", "gitleaks-installer": "exit 0"}.items():
                    target = (
                        installers / "install-gitleaks.sh"
                        if executable == "gitleaks-installer"
                        else binaries / executable
                    )
                    target.write_text("#!/usr/bin/env bash\n" + body + "\n")
                    target.chmod(0o700)
                analysis_installer = installers / "install-analysis-tools.sh"
                analysis_installer.write_text("#!/usr/bin/env bash\nprintf 'analysis-tools-ready\\n'\n")
                analysis_installer.chmod(0o700)
                for tool, version in versions["cargoSubcommands"].items():
                    if tool != "cargo-about":
                        target = binaries / tool
                        guard = (
                            '[[ "${1:-}" == llvm-cov && "${2:-}" == --version ]] || exit 2\n'
                            if tool == "cargo-llvm-cov"
                            else ""
                        )
                        target.write_text(f"#!/usr/bin/env bash\n{guard}echo '{tool} {version}'\n")
                        target.chmod(0o700)
                cargo = binaries / "cargo"
                cargo.write_text("""#!/usr/bin/env python3
import os, pathlib, subprocess, sys
args = sys.argv[1:]
if args[0] != 'install':
    sys.exit(subprocess.run([str(pathlib.Path(os.environ['FAKE_BIN']) / ('cargo-' + args[0])), *args]).returncode)
if '--features' in args and args[args.index('--features')+1] == 'cli' and os.environ['FAKE_MISSING'] != '1':
    target = pathlib.Path(os.environ['FAKE_BIN']) / 'cargo-about'
    version = args[args.index('--version') + 1]
    target.write_text('#!/usr/bin/env bash\\necho "cargo-about ' + version + '"\\n')
    target.chmod(0o700)
""")
                cargo.chmod(0o700)
                environment = {
                    **os.environ,
                    "PATH": str(binaries) + os.pathsep + "/usr/bin:/bin",
                    "FAKE_BIN": str(binaries),
                    "FAKE_MISSING": "1" if missing else "0",
                }
                result = subprocess.run(
                    ["bash", str(installers / "install-release-tools.sh")],
                    env=environment,
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, 1 if missing else 0, result.stderr)
                self.assertEqual("analysis-tools-ready" in result.stdout, not missing)


if __name__ == "__main__":
    unittest.main()
