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
# Exact Apache standard text: https://www.apache.org/licenses/LICENSE-2.0.txt.
# Compare the complete standard text while ignoring whitespace. Output bytes remain unchanged.
APACHE_2_STANDARD_SHA256 = "283ea6cc2997a1a70da0049e09adf9317bb60ca1b51279b65196b83a69e1996b"
APACHE_2_WORDS_SHA256 = "0ffddef9e48f8a09aed5caf2d44f7ba1c1be2d9b8e0a6f693b1635b2d5566645"
INPUT_PATHS = (
    "ui/package.json",
    "ui/package-lock.json",
    "ui/src/styles/fonts/golos-ui/golos-ui-vf.woff2",
    "ui/src/styles/fonts/golos-ui/LICENSE.txt",
    "tooling/qa/fixtures/licenses/apache-2.0.txt",
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
        normalized = " ".join(text.split()).encode("utf-8")
        if license_id == "Apache-2.0" and sha256_bytes(normalized) == APACHE_2_WORDS_SHA256:
            return
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


def web_license_inputs(root: Path = ROOT) -> dict[str, list]:
    """Use the locked production closure and preserve upstream license bytes."""
    ui = root / "ui"
    lock = json.loads((ui / "package-lock.json").read_text())
    manifest = json.loads((ui / "package.json").read_text())
    if lock.get("lockfileVersion") != 3 or lock["packages"][""]["dependencies"] != manifest["dependencies"]:
        raise ValueError("shared UI dependency lock does not match the manifest")
    result: dict[str, list] = {"crates": [], "licenses": []}
    # These exact Fluent archives omit their Apache license. The fixture is the
    # authoritative Apache text already pinned by the legal-policy tests.
    apache_fallbacks = {("@fluent/bundle", "0.19.1"), ("@fluent/langneg", "0.7.0"), ("@fluent/syntax", "0.19.0")}
    for path, entry in sorted(lock["packages"].items()):
        if not path or entry.get("dev"):
            continue
        directory = ui / path
        if (
            not path.startswith("node_modules/")
            or ".." in Path(path).parts
            or not directory.resolve().is_relative_to((ui / "node_modules").resolve())
        ):
            raise ValueError("shared UI package escapes its dependency directory")
        package = json.loads((directory / "package.json").read_text())
        name = path.rsplit("node_modules/", 1)[1]
        if package.get("name") != name or package.get("version") != entry.get("version") or entry.get("link"):
            raise ValueError("shared UI installed package identity differs from the lock")
        declared = package.get("license")
        selected = "MIT" if declared == "Apache-2.0 OR MIT" else declared
        if selected not in {"MIT", "Apache-2.0", "ISC"}:
            raise ValueError("shared UI dependency needs license review")
        names = (
            ["LICENSE-MIT", "LICENSE", "LICENSE.txt", "LICENSE.md"]
            if selected == "MIT"
            else ["LICENSE-APACHE-2.0", "LICENSE", "LICENSE.txt", "LICENSE.md"]
        )
        license_path = next((directory / name for name in names if (directory / name).is_file()), None)
        if license_path is not None:
            if not license_path.resolve().is_relative_to(directory.resolve()):
                raise ValueError("shared UI license escapes its package")
            content = license_path.read_bytes()
        elif selected == "Apache-2.0" and (name, entry["version"]) in apache_fallbacks:
            content = (root / "tooling/qa/fixtures/licenses/apache-2.0.txt").read_bytes()
            if sha256_bytes(content.decode().strip().replace("\r\n", "\n").encode()) != APACHE_2_STANDARD_SHA256:
                raise ValueError("shared UI Apache clarification changed")
        else:
            raise ValueError("shared UI package has no authoritative license text")
        source = entry.get("resolved", "")
        if not source.startswith("https://registry.npmjs.org/") or not entry.get("integrity", "").startswith("sha512-"):
            raise ValueError("shared UI package lacks locked registry provenance")
        package = {
            "id": f"npm:{name}@{entry['version']}",
            "name": name,
            "version": entry["version"],
            "source": source,
            "repository": None,
            "license": declared,
        }
        result["crates"].append({"package": package})
        result["licenses"].append({"id": selected, "text": content.decode("utf-8"), "used_by": [{"crate": package}]})
    font = ui / "src/styles/fonts/golos-ui/golos-ui-vf.woff2"
    identity = sha256_bytes(font.read_bytes())
    package = {
        "id": f"font:golos-ui:{identity}",
        "name": "golos-ui",
        "version": identity,
        "source": f"bundled-font:{identity}",
        "repository": None,
        "license": "OFL-1.1",
    }
    result["crates"].append({"package": package})
    result["licenses"].append(
        {
            "id": "OFL-1.1",
            "text": (font.parent / "LICENSE.txt").read_bytes().decode("utf-8"),
            "used_by": [{"crate": package}],
        }
    )
    return result


def render_notices(raw: dict[str, Any], root: Path = ROOT) -> bytes:
    parts = [(root / "tooling/release/legal/third-party-notices.hbs").read_bytes(), b"\n"]
    for entry in raw["licenses"]:
        reject_placeholder_license(entry["text"], entry["id"])
        names = ", ".join(f"{item['crate']['name']} {item['crate']['version']}" for item in entry["used_by"])
        parts.append(f"## {entry.get('name', entry['id'])}\n\nUsed by: {names}\n\n```text\n".encode())
        parts.append(entry["text"].encode("utf-8"))
        parts.append(b"\n```\n\n")
    return b"".join(parts)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate deterministic dependency-license files from cargo-about JSON."
    )
    parser.add_argument("--cargo-about-json", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args()
    raw = json.loads(args.cargo_about_json.read_text(encoding="utf-8"))
    web = web_license_inputs()
    for field in ("crates", "licenses"):
        raw[field].extend(web[field])
    manifest, license_files = build_outputs(raw)
    write_outputs(args.output_dir, manifest, license_files)
    (args.output_dir / "THIRD_PARTY_NOTICES.md").write_bytes(render_notices(raw))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
