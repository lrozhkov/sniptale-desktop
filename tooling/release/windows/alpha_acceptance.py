"""Export a bounded public summary only after verifying complete local VM evidence."""
from __future__ import annotations

import argparse
import base64
import json
import subprocess
from pathlib import Path

from alpha_artifacts import ROOT, digest, read_json, verify_bundle, write_json
from alpha_publish import RELEASE_CASES, admit, api


def export(bundle: Path, run_directory: Path, artifact_id: int, output: Path) -> None:
    run_directory = run_directory.resolve(strict=True)
    lab = (ROOT / ".tmp/qa/desktop-e2e-lab").resolve()
    if not run_directory.is_relative_to(lab) or run_directory.parent != lab:
        raise ValueError("acceptance receipt must belong to the local lab")
    subprocess.run([str(ROOT / "tooling/desktop/e2e-lab"), "collect", run_directory.name], check=True)
    receipt = read_json(run_directory / "receipt.json")
    provenance = read_json(bundle / "provenance.json")
    commit, tag = provenance["commit"], provenance["tag"]
    verify_bundle(bundle, commit, tag)
    tests = {item["name"]: item["status"] for lane in receipt["test_matrix"] for item in lane["tests"]}
    if (receipt.get("result") != "pass" or receipt.get("run_mode") != "alpha-release-acceptance"
            or receipt.get("commit_sha") != commit or tests != {name: "pass" for name in RELEASE_CASES}
            or any(any(values.values()) for values in receipt["teardown"].values())):
        raise ValueError("VM receipt is not complete clean alpha acceptance")
    recorded = {item["name"]: item["sha256"] for item in receipt["artifacts"]}
    for path in bundle.rglob("*"):
        if path.is_file() and recorded.get(path.name) != digest(path):
            raise ValueError("VM receipt artifact hashes differ from candidate")
    run = api(f"actions/runs/{provenance['runId']}")
    artifact = api(f"actions/artifacts/{artifact_id}")
    value = {
        "schemaVersion": 1, "kind": "sniptale-alpha-windows-acceptance", "commit": commit, "tag": tag,
        "runId": provenance["runId"], "runAttempt": provenance["runAttempt"],
        "artifactId": artifact_id, "artifactDigest": artifact["digest"], "result": "pass",
        "verified": True, "cleanTeardown": True, "devLocalTrust": False, "tests": tests,
        "checksumsSha256": digest(bundle / "SHA256SUMS"), "receiptSha256": digest(run_directory / "receipt.json"),
        "windowsImage": receipt["snapshots"]["windows-11"],
    }
    admit(run, artifact, value, commit, tag)
    write_json(output, value)
    print(base64.b64encode(json.dumps(value, separators=(",", ":")).encode()).decode())


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("bundle", type=Path)
    parser.add_argument("run_directory", type=Path)
    parser.add_argument("artifact_id", type=int)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    export(args.bundle, args.run_directory, args.artifact_id, args.output)
