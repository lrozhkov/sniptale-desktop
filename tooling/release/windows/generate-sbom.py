#!/usr/bin/env python3
from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import os
import subprocess
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]


def spdx_id(value: str) -> str:
    safe = "".join(char if char.isalnum() or char in ".-" else "-" for char in value)
    return "SPDXRef-Package-" + safe.strip("-")


def cargo_metadata() -> dict:
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--filter-platform", "x86_64-pc-windows-msvc", "--all-features"],
        cwd=ROOT,
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def package_key(package: dict) -> tuple[str, str, str | None]:
    return package["name"], package["version"], package.get("source")


def package_spdx_id(package: dict) -> str:
    identity_digest = hashlib.sha256(package["id"].encode("utf-8")).hexdigest()[:12]
    return spdx_id(f"{package['name']}-{package['version']}-{identity_digest}")


def package_entry(package: dict, declared_license: str | None = None) -> dict:
    name = package["name"]
    version = package["version"]
    return {
        "name": name,
        "SPDXID": package_spdx_id(package),
        "versionInfo": version,
        "downloadLocation": package.get("source") or "NOASSERTION",
        "filesAnalyzed": False,
        "licenseConcluded": "NOASSERTION",
        "licenseDeclared": declared_license or package.get("license") or "NOASSERTION",
        "copyrightText": "NOASSERTION",
        "supplier": "NOASSERTION",
    }


def created_at() -> str:
    raw_epoch = os.environ.get("SOURCE_DATE_EPOCH")
    if raw_epoch is None:
        result = subprocess.run(
            ["git", "show", "-s", "--format=%ct", "HEAD"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        )
        raw_epoch = result.stdout.strip()
    return dt.datetime.fromtimestamp(int(raw_epoch), dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def source_revision() -> str:
    result = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, check=True, capture_output=True, text=True)
    return result.stdout.strip()


def build_sbom(metadata: dict, legal_manifest: dict, manifest_digest: str, revision: str, created: str) -> dict:
    metadata_by_key: dict[tuple[str, str, str | None], list[dict]] = {}
    for package in metadata["packages"]:
        metadata_by_key.setdefault(package_key(package), []).append(package)

    selected: list[tuple[dict, str | None]] = []
    selected_ids: set[str] = set()
    for record in legal_manifest["packages"]:
        key = package_key(record)
        matches = metadata_by_key.get(key, [])
        if len(matches) != 1:
            raise ValueError(f"dependency legal manifest package must resolve once in Cargo metadata: {key!r}")
        package = matches[0]
        selected.append((package, record.get("declaredLicense")))
        selected_ids.add(package["id"])

    workspace_members = set(metadata.get("workspace_members", []))
    roots = [package for package in metadata["packages"] if package["id"] in workspace_members]
    if len(roots) != 1:
        raise ValueError("SBOM generation requires exactly one workspace root package")
    root = roots[0]
    if root["id"] in selected_ids:
        raise ValueError("first-party workspace package must not appear in the third-party legal manifest")

    selected.sort(key=lambda item: package_key(item[0]))
    packages = [package_entry(root)] + [package_entry(package, license_value) for package, license_value in selected]
    relationships = [
        {
            "spdxElementId": "SPDXRef-DOCUMENT",
            "relationshipType": "DESCRIBES",
            "relatedSpdxElement": package_spdx_id(root),
        }
    ]
    packages_by_id = {package["id"]: package for package, _ in selected}
    packages_by_id[root["id"]] = root
    nodes = {node["id"]: node for node in metadata.get("resolve", {}).get("nodes", [])}
    if set(packages_by_id) - set(nodes):
        raise ValueError("Cargo metadata resolve graph is missing packages selected for the SBOM")
    dependency_edges: set[tuple[str, str]] = set()
    for package_id, package in packages_by_id.items():
        for dependency in nodes[package_id].get("deps", []):
            dependency_id = dependency["pkg"]
            if dependency_id in packages_by_id:
                dependency_edges.add((package_spdx_id(package), package_spdx_id(packages_by_id[dependency_id])))
    relationships.extend(
        {"spdxElementId": source, "relationshipType": "DEPENDS_ON", "relatedSpdxElement": target}
        for source, target in sorted(dependency_edges)
    )
    return {
        "spdxVersion": "SPDX-2.3",
        "dataLicense": "CC0-1.0",
        "SPDXID": "SPDXRef-DOCUMENT",
        "name": "sniptale-desktop-windows-runtime-cargo-sbom",
        "documentNamespace": f"https://sniptale.invalid/spdx/sniptale-desktop/{revision}/{manifest_digest}",
        "creationInfo": {
            "created": created,
            "creators": ["Tool: tooling/release/windows/generate-sbom.py"],
        },
        "documentDescribes": [package_spdx_id(root)],
        "packages": packages,
        "relationships": relationships,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Generate an SPDX SBOM for the collected Windows runtime dependency closure")
    parser.add_argument("--out", default=".tmp/release/sbom.spdx.json", help="SBOM output path")
    parser.add_argument("--legal-manifest", default="THIRD_PARTY_DEPENDENCIES.json", help="Tracked dependency legal manifest")
    args = parser.parse_args()
    manifest_path = ROOT / args.legal_manifest
    manifest_bytes = manifest_path.read_bytes()
    legal_manifest = json.loads(manifest_bytes)
    output = ROOT / args.out
    output.parent.mkdir(parents=True, exist_ok=True)
    sbom = build_sbom(cargo_metadata(), legal_manifest, hashlib.sha256(manifest_bytes).hexdigest(), source_revision(), created_at())
    output.write_text(json.dumps(sbom, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"SBOM written: {output.relative_to(ROOT).as_posix()}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
