from __future__ import annotations

import importlib.util
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tooling/release/windows"))
import alpha_artifacts as artifacts
import alpha_publish as publish


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


if __name__ == "__main__":
    unittest.main()
