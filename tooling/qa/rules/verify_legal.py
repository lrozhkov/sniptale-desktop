#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
POLICY = ROOT / "tooling/configs/qa/open-source.json"
MANIFEST = ROOT / "THIRD_PARTY_DEPENDENCIES.json"
NOTICE_BUNDLE = ROOT / "THIRD_PARTY_NOTICES.md"
LICENSE_DIR = ROOT / "LICENSES/third-party"
EXPECTED_INPUTS = {
    "Cargo.toml",
    "Cargo.lock",
    "deny.toml",
    "tooling/configs/qa/about.toml",
    "tooling/configs/qa/open-source.json",
    "tooling/configs/qa/tool-versions.json",
    "tooling/release/legal/third-party-notices.hbs",
    "tooling/release/legal/generate_dependency_licenses.py",
    "tooling/qa/wrappers/licenses.sh",
}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def relative_files(root: Path) -> set[str]:
    if not root.is_dir():
        return set()
    return {path.relative_to(ROOT).as_posix() for path in root.rglob("*") if path.is_file()}


def safe_component(value: str) -> str:
    return re.sub(r"[^A-Za-z0-9._-]+", "_", value).strip("._-")


def readable_license_path_errors(manifest: dict) -> list[str]:
    errors: list[str] = []
    record_paths = {
        record.get("file") for record in manifest.get("licenseFiles", []) if isinstance(record, dict)
    }
    selected_paths: set[str] = set()
    for package in manifest.get("packages", []):
        if not isinstance(package, dict):
            errors.append("dependency package record must be an object")
            continue
        name = package.get("name")
        version = package.get("version")
        selected = package.get("selectedLicenses")
        if not isinstance(name, str) or not isinstance(version, str) or not isinstance(selected, list) or not selected:
            errors.append("dependency package record has invalid identity or selected licenses")
            continue
        base = f"LICENSES/third-party/{safe_component(name)}-{safe_component(version)}"
        for license_record in selected:
            if not isinstance(license_record, dict) or not isinstance(license_record.get("spdxId"), str):
                errors.append(f"dependency package has an invalid selected license: {name} {version}")
                continue
            suffix = "" if len(selected) == 1 else f"-{safe_component(license_record['spdxId'])}"
            if sum(isinstance(item, dict) and item.get("spdxId") == license_record["spdxId"] for item in selected) > 1:
                sha = license_record.get("sha256")
                if not isinstance(sha, str) or re.fullmatch(r"[0-9a-f]{64}", sha) is None:
                    errors.append("duplicate SPDX texts require exact content hashes")
                    continue
                suffix += "-" + sha[:12]
            expected = f"{base}{suffix}.txt"
            actual = license_record.get("file")
            if actual != expected:
                errors.append(f"dependency license filename must identify its package: {actual}")
            if isinstance(actual, str):
                selected_paths.add(actual)
    if selected_paths != record_paths:
        errors.append("dependency package license references differ from license file records")
    return errors


def contribution_policy_errors(policy: dict, contributing: str) -> list[str]:
    errors: list[str] = []
    if policy.get("project", {}).get("contributionsAccepted") is not False:
        errors.append("open-source policy must disable external code contributions")
    lowered = contributing.lower()
    if "does not currently accept external code contributions" not in lowered:
        errors.append("CONTRIBUTING must reject external code contributions")
    if "welcome external code contributions" in lowered or "accepts external code contributions" in lowered:
        errors.append("CONTRIBUTING contains contradictory contribution policy")
    return errors


def validate_static() -> list[str]:
    errors: list[str] = []
    required = [POLICY, ROOT / "LICENSE", ROOT / "NOTICE", ROOT / "CONTRIBUTING.md", MANIFEST, NOTICE_BUNDLE]
    missing = [path.relative_to(ROOT).as_posix() for path in required if not path.is_file()]
    if missing:
        return ["missing legal files: " + ", ".join(missing)]

    policy = json.loads(POLICY.read_text(encoding="utf-8"))
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]
    deny = tomllib.loads((ROOT / "deny.toml").read_text(encoding="utf-8"))
    tools = json.loads((ROOT / "tooling/configs/qa/tool-versions.json").read_text(encoding="utf-8"))
    if cargo.get("name") != policy["project"]["cargoPackage"] or cargo.get("license") != policy["project"]["license"]:
        errors.append("Cargo package identity/license differs from open-source policy")
    if cargo.get("publish") is not False:
        errors.append("Cargo publish must stay false to prevent accidental registry publication")
    license_policy = deny.get("licenses", {})
    if license_policy.get("include-build") is not True or license_policy.get("include-dev") is not True:
        errors.append("cargo-deny license audit must include build and dev dependencies")
    if license_policy.get("private", {}).get("ignore") is not False:
        errors.append("cargo-deny must audit the private first-party crate")
    if "cargoAboutVersion" in policy["dependencyLicenses"]:
        errors.append("cargo-about version must be owned only by tool-versions.json")

    for relative, expected in policy.get("trackedDigests", {}).items():
        path = ROOT / relative
        if not path.is_file() or digest(path) != expected:
            errors.append(f"tracked legal digest drift: {relative}")
    notice = (ROOT / "NOTICE").read_text(encoding="utf-8")
    if policy["project"]["name"] not in notice or policy["project"]["copyright"] not in notice:
        errors.append("NOTICE differs from project identity")
    contributing = (ROOT / "CONTRIBUTING.md").read_text(encoding="utf-8")
    errors.extend(contribution_policy_errors(policy, contributing))

    manifest = json.loads(MANIFEST.read_text(encoding="utf-8"))
    if manifest.get("generator") != {"name": "cargo-about", "version": tools["cargoSubcommands"]["cargo-about"]}:
        errors.append("dependency legal generator differs from canonical tool version")
    if manifest.get("formatVersion") != 1:
        errors.append("dependency legal manifest formatVersion must be 1")
    inputs = manifest.get("inputDigests", {})
    if set(inputs) != EXPECTED_INPUTS:
        errors.append("dependency legal manifest input set is stale")
    else:
        for relative, expected in inputs.items():
            path = ROOT / relative
            if not path.is_file() or digest(path) != expected:
                errors.append(f"dependency legal input digest drift: {relative}")
    records = manifest.get("licenseFiles", [])
    errors.extend(readable_license_path_errors(manifest))
    expected_files = {record.get("file") for record in records if isinstance(record, dict)}
    actual_files = relative_files(LICENSE_DIR)
    if expected_files != actual_files:
        errors.append("dependency license file inventory differs from manifest")
    for record in records:
        if not isinstance(record, dict):
            errors.append("dependency license record must be an object")
            continue
        path = ROOT / str(record.get("file"))
        if not path.is_file() or digest(path) != record.get("sha256"):
            errors.append(f"dependency license digest drift: {record.get('file')}")
        elif any(marker in path.read_text(encoding="utf-8").lower() for marker in ("<year>", "<copyright holders>", "<copyright holder>", "[year]", "[yyyy]")):
            errors.append(f"dependency license contains unresolved placeholders: {record.get('file')}")
    if not manifest.get("packages"):
        errors.append("dependency legal manifest contains no packages")
    return errors


def validate_candidate(candidate: Path) -> list[str]:
    errors: list[str] = []
    for relative in ("THIRD_PARTY_DEPENDENCIES.json", "THIRD_PARTY_NOTICES.md"):
        expected = ROOT / relative
        actual = candidate / relative
        if not actual.is_file() or not expected.is_file() or actual.read_bytes() != expected.read_bytes():
            errors.append(f"generated legal output is stale: {relative}")
    expected_files = relative_files(LICENSE_DIR)
    candidate_root = candidate / "LICENSES/third-party"
    candidate_files = {
        path.relative_to(candidate).as_posix() for path in candidate_root.rglob("*") if path.is_file()
    } if candidate_root.is_dir() else set()
    if expected_files != candidate_files:
        errors.append("generated dependency license inventory is stale")
    else:
        for relative in expected_files:
            if (ROOT / relative).read_bytes() != (candidate / relative).read_bytes():
                errors.append(f"generated dependency license is stale: {relative}")
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate Sniptale open-source and dependency legal policy.")
    parser.add_argument("--candidate", type=Path)
    args = parser.parse_args()
    errors = validate_static()
    if args.candidate is not None:
        errors.extend(validate_candidate(args.candidate))
    if errors:
        for error in errors:
            print(f"FAIL {error}", file=sys.stderr)
        return 1
    print("OK open-source and dependency legal policy")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
