"""Bounded alpha artifact and cross-job QA admission. No downloaded code is executed."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
ORIGIN = "chrome-extension://pcegkgbagkiagakmfjchgfgheepkllda/"
PROOF_FILES = {"sbom.spdx.json", "cargo-audit.json", "cargo-deny.txt", "qa-proof.json"}
GATES = {
    "qa:release-harness",
    "qa:checkpoint",
    "qa:coverage",
    "qa:supply-chain",
    "qa:audit",
    "qa:release",
    "qa:fast",
    "qa:script-lints",
    "qa:environment-audit",
}


def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def read_json(path: Path) -> dict:
    if path.is_symlink() or not path.is_file() or path.stat().st_size > 4 * 1024 * 1024:
        raise ValueError("missing, redirected or oversized release JSON")
    value = json.loads(path.read_text(encoding="utf-8-sig"))
    if not isinstance(value, dict):
        raise ValueError("release JSON must be an object")
    return value


def write_json(path: Path, value: dict) -> None:
    path.write_text(json.dumps(value, sort_keys=True, indent=2) + "\n", encoding="utf-8")


def files(directory: Path) -> dict[str, str]:
    result = {}
    for path in sorted(directory.rglob("*")):
        if path.is_symlink():
            raise ValueError("release symlinks are forbidden")
        if path.is_file():
            name = path.relative_to(directory).as_posix()
            if not re.fullmatch(r"[A-Za-z0-9_.\-/]+", name) or path.stat().st_size > 256 * 1024 * 1024:
                raise ValueError("invalid release file name or size")
            result[name] = digest(path)
    if len(result) > 512:
        raise ValueError("too many release files")
    return result


def assert_source(commit: str) -> None:
    if not re.fullmatch(r"[0-9a-f]{40}", commit) or git("rev-parse", "HEAD") != commit:
        raise ValueError("release source commit mismatch")
    if git("status", "--porcelain"):
        raise ValueError("release requires a clean source checkout")


def version_tag(tag: str) -> str:
    match = re.fullmatch(r"v(0\.\d+\.\d+)-alpha\.([1-9][0-9]*)", tag)
    if not match:
        raise ValueError("expected v0.X.Y-alpha.N")
    return match[1]


def make_proof(output: Path) -> None:
    commit = git("rev-parse", "HEAD")
    assert_source(commit)
    accepted = {}
    for path in sorted((ROOT / ".tmp/qa-observability/runs").glob("**/*.json")):
        record = read_json(path)
        owner = record.get("wrapperId")
        source = record.get("repository", {})
        if (
            owner in GATES
            and record.get("status") == "all-passed"
            and record.get("exitCode") == 0
            and source.get("revision") == commit
            and source.get("targetFiles") == []
        ):
            accepted[owner] = {"runId": record["runId"], "recordSha256": digest(path)}
    if set(accepted) != GATES:
        raise ValueError("missing clean-commit canonical release gates")
    output.mkdir(parents=True, exist_ok=False)
    for source, name in [
        (".tmp/release/sbom.spdx.json", "sbom.spdx.json"),
        (".tmp/release/supply-chain/cargo-audit.json", "cargo-audit.json"),
        (".tmp/release/supply-chain/cargo-deny.txt", "cargo-deny.txt"),
    ]:
        shutil.copyfile(ROOT / source, output / name)
    write_json(
        output / "qa-proof.json",
        {
            "schemaVersion": 1,
            "kind": "sniptale-alpha-qa",
            "commit": commit,
            "tree": git("rev-parse", "HEAD^{tree}"),
            "gates": accepted,
            "files": files(output),
            "runId": os.environ.get("GITHUB_RUN_ID", "local"),
            "runAttempt": os.environ.get("GITHUB_RUN_ATTEMPT", "local"),
        },
    )


def verify_proof(directory: Path, commit: str) -> dict:
    proof = read_json(directory / "qa-proof.json")
    actual = files(directory)
    actual.pop("qa-proof.json", None)
    if (
        proof.get("schemaVersion") != 1
        or proof.get("kind") != "sniptale-alpha-qa"
        or proof.get("commit") != commit
        or set(files(directory)) != PROOF_FILES
        or proof.get("files") != actual
        or set(proof.get("gates", {})) != GATES
    ):
        raise ValueError("QA handoff identity, gates or hashes mismatch")
    if not re.fullmatch(r"[0-9a-f]{40}", proof.get("tree", "")):
        raise ValueError("QA source tree is malformed")
    for gate in proof["gates"].values():
        if (
            not isinstance(gate, dict)
            or not isinstance(gate.get("runId"), str)
            or not re.fullmatch(r"[0-9a-f]{64}", gate.get("recordSha256", ""))
        ):
            raise ValueError("QA gate receipt is malformed")
    return proof


def seal(directory: Path, commit: str, tag: str) -> None:
    version = version_tag(tag)
    assert_source(commit)
    proof = verify_proof(directory / "qa", commit)
    if proof["tree"] != git("rev-parse", "HEAD^{tree}"):
        raise ValueError("QA source tree mismatch")
    if proof.get("runId") != os.environ.get("GITHUB_RUN_ID", "local") or proof.get("runAttempt") != os.environ.get(
        "GITHUB_RUN_ATTEMPT", "local"
    ):
        raise ValueError("QA producer run or attempt mismatch")
    write_json(
        directory / "provenance.json",
        {
            "schemaVersion": 1,
            "kind": "sniptale-alpha-package",
            "commit": commit,
            "tree": proof["tree"],
            "tag": tag,
            "productVersion": version,
            "channel": "dev",
            "signingMode": "unsigned",
            "devLocalTrust": False,
            "extensionOrigin": ORIGIN,
            "scope": "manual-only",
            "target": "x86_64-pc-windows-msvc",
            "runId": os.environ.get("GITHUB_RUN_ID", "local"),
            "runAttempt": os.environ.get("GITHUB_RUN_ATTEMPT", "local"),
            "repository": "lrozhkov/sniptale-desktop",
            "files": files(directory),
        },
    )
    inventory = files(directory)
    (directory / "SHA256SUMS").write_text(
        "".join(f"{value}  {name}\n" for name, value in inventory.items()), encoding="utf-8"
    )
    verify_bundle(directory, commit, tag)


def verify_bundle(directory: Path, commit: str, tag: str) -> dict:
    version = version_tag(tag)
    if tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"] != version:
        raise ValueError("alpha version differs from reviewed Cargo version")
    provenance = read_json(directory / "provenance.json")
    required = {
        "schemaVersion": 1,
        "kind": "sniptale-alpha-package",
        "commit": commit,
        "tag": tag,
        "productVersion": version,
        "channel": "dev",
        "signingMode": "unsigned",
        "devLocalTrust": False,
        "extensionOrigin": ORIGIN,
        "scope": "manual-only",
        "repository": "lrozhkov/sniptale-desktop",
        "target": "x86_64-pc-windows-msvc",
    }
    if any(provenance.get(key) != value for key, value in required.items()):
        raise ValueError("alpha provenance identity or policy mismatch")
    actual = files(directory)
    actual.pop("SHA256SUMS", None)
    lines = (directory / "SHA256SUMS").read_text(encoding="utf-8").splitlines()
    expected = {}
    for line in lines:
        match = re.fullmatch(r"([0-9a-f]{64})  ([A-Za-z0-9_.\-/]+)", line)
        if not match or match[2] in expected:
            raise ValueError("invalid or duplicate checksum entry")
        expected[match[2]] = match[1]
    if expected != actual:
        raise ValueError("release checksum inventory mismatch")
    actual.pop("provenance.json", None)
    if provenance.get("files") != actual:
        raise ValueError("release provenance file inventory mismatch")
    proof = verify_proof(directory / "qa", commit)
    if proof["tree"] != provenance.get("tree"):
        raise ValueError("QA/package tree mismatch")
    if proof.get("runId") != provenance.get("runId") or proof.get("runAttempt") != provenance.get("runAttempt"):
        raise ValueError("QA/package producer identity mismatch")
    allowed = {
        f"SniptaleNativeAgent-dev-{version}.msi",
        "sniptale-native-agent.exe",
        "native-host.json",
        "LICENSE.txt",
        "NOTICE.txt",
        "THIRD_PARTY_NOTICES.md",
        "THIRD_PARTY_DEPENDENCIES.json",
        "build-environment.json",
        *[f"qa/{name}" for name in PROOF_FILES],
    }
    for source, destination in [
        ("LICENSE", "LICENSE.txt"),
        ("NOTICE", "NOTICE.txt"),
        ("THIRD_PARTY_NOTICES.md", "THIRD_PARTY_NOTICES.md"),
        ("THIRD_PARTY_DEPENDENCIES.json", "THIRD_PARTY_DEPENDENCIES.json"),
    ]:
        if actual.get(destination) != digest(ROOT / source):
            raise ValueError("release legal bytes mismatch")
    for source in (ROOT / "LICENSES/third-party").glob("*"):
        name = "LICENSES/third-party/" + source.name
        allowed.add(name)
        if actual.get(name) != digest(source):
            raise ValueError("third-party license bytes mismatch")
    if set(actual) != allowed:
        raise ValueError("unexpected or missing alpha assets")
    manifest = read_json(directory / "native-host.json")
    if (
        manifest.get("name") != "com.sniptale.dev.native_host"
        or manifest.get("allowed_origins") != [ORIGIN]
        or manifest.get("path") != "sniptale-native-agent.exe"
    ):
        raise ValueError("alpha manifest identity mismatch")
    return provenance


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=("proof", "verify-proof", "seal", "verify"))
    parser.add_argument("directory", type=Path)
    parser.add_argument("--commit", default="")
    parser.add_argument("--tag", default="")
    args = parser.parse_args()
    if args.mode == "proof":
        make_proof(args.directory)
    elif args.mode == "verify-proof":
        verify_proof(args.directory, args.commit)
    elif args.mode == "seal":
        seal(args.directory, args.commit, args.tag)
    else:
        verify_bundle(args.directory, args.commit, args.tag)
    print("alpha artifact verification passed")


if __name__ == "__main__":
    main()
