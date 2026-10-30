#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import re
from collections import Counter, defaultdict
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[3]
INPUT_PATHS = (
    "Cargo.toml",
    "Cargo.lock",
    "deny.toml",
    "tooling/configs/qa/about.toml",
    "tooling/configs/qa/open-source.json",
    "tooling/configs/qa/tool-versions.json",
    "tooling/release/legal/third-party-notices.hbs",
    "tooling/release/legal/generate_dependency_licenses.py",
    "tooling/qa/wrappers/licenses.sh",
)


def sha256_bytes(value: bytes) -> str:
    return hashlib.sha256(value).hexdigest()


def reject_placeholder_license(text: str, license_id: str) -> None:
    lowered = text.lower()
    placeholders = ("<year>", "<copyright holders>", "<copyright holder>", "[year]", "[yyyy]")
    found = [placeholder for placeholder in placeholders if placeholder in lowered]
    if found:
        raise ValueError(f"cargo-about returned placeholder license text for {license_id}: {', '.join(found)}")


def input_digests(root: Path = ROOT) -> dict[str, str]:
    return {path: sha256_bytes((root / path).read_bytes()) for path in INPUT_PATHS}


def safe_component(value: str) -> str:
    normalized = re.sub(r"[^A-Za-z0-9._-]+", "_", value).strip("._-")
    if not normalized:
        raise ValueError(f"cannot form a readable license filename from {value!r}")
    return normalized


def build_outputs(raw: dict[str, Any], root: Path = ROOT) -> tuple[dict[str, Any], dict[str, bytes]]:
    policy = json.loads((root / "tooling/configs/qa/open-source.json").read_text(encoding="utf-8"))
    tools = json.loads((root / "tooling/configs/qa/tool-versions.json").read_text(encoding="utf-8"))
    packages = {entry["package"]["id"]: entry for entry in raw.get("crates", [])}
    package_licenses: dict[str, list[dict[str, Any]]] = defaultdict(list)
    license_files: dict[str, bytes] = {}

    for license_entry in raw.get("licenses", []):
        text = license_entry.get("text")
        license_id = license_entry.get("id")
        if not isinstance(text, str) or not text or not isinstance(license_id, str) or not license_id:
            raise ValueError("cargo-about returned an empty license id or text")
        reject_placeholder_license(text, license_id)
        content = text.encode("utf-8")
        digest = sha256_bytes(content)
        for usage in license_entry.get("used_by", []):
            package_id = usage.get("crate", {}).get("id")
            if package_id not in packages:
                raise ValueError(f"cargo-about license usage references an unknown package: {package_id}")
            package_licenses[package_id].append({"spdxId": license_id, "sha256": digest, "content": content})

    package_records = []
    license_records = []
    package_bases: set[str] = set()
    for package_id, entry in packages.items():
        package = entry["package"]
        distinct_sources = {(item["spdxId"], item["sha256"]): item for item in package_licenses.get(package_id, [])}
        selected_sources = [distinct_sources[key] for key in sorted(distinct_sources)]
        spdx_counts = Counter(item["spdxId"] for item in selected_sources)
        if not selected_sources:
            raise ValueError(f"cargo-about returned no selected license text for {package_id}")
        base = f"{safe_component(package['name'])}-{safe_component(package['version'])}"
        if base in package_bases:
            raise ValueError(f"multiple dependency identities map to the same license filename: {base}")
        package_bases.add(base)
        selected = []
        for source in selected_sources:
            suffix = "" if len(selected_sources) == 1 else f"-{safe_component(source['spdxId'])}"
            if spdx_counts[source["spdxId"]] > 1:
                suffix += "-" + source["sha256"][:12]
            relative = f"LICENSES/third-party/{base}{suffix}.txt"
            if relative in license_files:
                raise ValueError(f"multiple selected license texts map to the same filename: {relative}")
            license_files[relative] = source["content"]
            selected.append({"spdxId": source["spdxId"], "file": relative, "sha256": source["sha256"]})
            license_records.append(
                {
                    "file": relative,
                    "sha256": source["sha256"],
                    "spdxIds": [source["spdxId"]],
                    "packages": [f"{package['name']} {package['version']}"],
                }
            )
        package_records.append(
            {
                "name": package["name"],
                "version": package["version"],
                "source": package.get("source"),
                "repository": package.get("repository"),
                "declaredLicense": package.get("license"),
                "selectedLicenses": selected,
            }
        )

    manifest = {
        "formatVersion": 1,
        "generator": {"name": "cargo-about", "version": tools["cargoSubcommands"]["cargo-about"]},
        "scope": {
            "target": policy["dependencyLicenses"]["target"],
            "allFeatures": policy["dependencyLicenses"]["allFeatures"],
            "includeBuildOnly": policy["dependencyLicenses"]["includeBuildOnly"],
            "includeDevOnly": policy["dependencyLicenses"]["includeDevOnly"],
            "includeTransitive": policy["dependencyLicenses"]["includeTransitive"],
        },
        "inputDigests": input_digests(root),
        "packages": sorted(package_records, key=lambda item: (item["name"], item["version"], item["source"] or "")),
        "licenseFiles": sorted(license_records, key=lambda item: item["file"]),
    }
    return manifest, license_files


def write_outputs(output_dir: Path, manifest: dict[str, Any], license_files: dict[str, bytes]) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    for relative, content in license_files.items():
        path = output_dir / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)
    (output_dir / "THIRD_PARTY_DEPENDENCIES.json").write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n", encoding="utf-8"
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate deterministic dependency-license files from cargo-about JSON."
    )
    parser.add_argument("--cargo-about-json", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()
    raw = json.loads(args.cargo_about_json.read_text(encoding="utf-8"))
    manifest, license_files = build_outputs(raw)
    write_outputs(args.output_dir, manifest, license_files)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
