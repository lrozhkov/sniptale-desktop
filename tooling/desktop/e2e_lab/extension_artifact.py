"""Package a real extension runtime with an explicit, isolated lab identity."""

from __future__ import annotations

import json
import re
import stat
import zipfile
from pathlib import Path

from artifacts import source_tree_fingerprint
from errors import IntegrityError
from state import read_json, write_json
from system import run, sha256_file


def artifact_files(artifact):
    entries = []
    total = 0
    names = set()
    for file in artifact.rglob("*"):
        mode = file.lstat().st_mode
        if stat.S_ISDIR(mode):
            continue
        if not stat.S_ISREG(mode):
            raise IntegrityError("extension artifact contains a link or special file")
        if file.suffix == ".map":
            continue
        relative = file.relative_to(artifact).as_posix()
        if (
            "\\" in relative
            or ":" in relative
            or any(part.endswith((".", " ")) for part in file.relative_to(artifact).parts)
            or relative.casefold() in names
        ):
            raise IntegrityError("extension artifact path is unsafe on Windows")
        names.add(relative.casefold())
        size = file.stat().st_size
        total += size
        if len(entries) >= 5000 or size > 64 * 1024 * 1024 or total > 256 * 1024 * 1024:
            raise IntegrityError("extension artifact exceeds transfer budget")
        entries.append({"name": file.relative_to(artifact).as_posix(), "sha256": sha256_file(file), "bytes": size})
    return sorted(entries, key=lambda item: item["name"])


def package_extension(paths, artifact: Path, source_root: Path, output: Path):
    artifact = artifact.resolve(strict=False)
    source_root = source_root.resolve(strict=True)
    if artifact != source_root / ".tmp/e2e-builds/test":
        raise IntegrityError("real extension proof requires the owner's test-e2e build output")
    if not (source_root / ".git").exists():
        raise IntegrityError("extension source must be a Git checkout")
    run(
        [
            "node",
            str(paths.root / "tooling/desktop/e2e_lab/tests/real-extension-contracts.cjs"),
            str(paths.root / "tooling/desktop/e2e_lab/guest/windows/real-extension.js"),
        ],
        cwd=paths.root,
        timeout=30,
        log=output / "logs/real-extension-contracts.log",
    )
    run(
        [
            "node",
            str(paths.root / "tooling/desktop/e2e_lab/tests/browser-playwright-contracts.cjs"),
            str(paths.root / "tooling/desktop/e2e_lab/guest/windows/browser-playwright.cjs"),
        ],
        cwd=paths.root,
        timeout=30,
        log=output / "logs/browser-playwright-contracts.log",
    )
    source = source_tree_fingerprint(source_root)
    stamp = paths.scoped(paths.cache / "session-dependencies/extension-build.json")
    previous = read_json(stamp)
    current = artifact_files(artifact)
    if previous is None or previous.get("source") != source or previous.get("artifacts") != current:
        package = read_json(source_root / "package.json") or {}
        try:
            npm_version = package["devEngines"]["packageManager"]["version"]
        except (KeyError, TypeError) as exc:
            raise IntegrityError("extension npm version must be pinned") from exc
        if not isinstance(npm_version, str) or re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", npm_version) is None:
            raise IntegrityError("extension npm version must be pinned")
        run(
            [
                "npx",
                "--yes",
                f"--package=npm@{npm_version}",
                "npm",
                "--prefix",
                str(source_root),
                "run",
                "qa:e2e:build",
            ],
            cwd=paths.root,
            timeout=600,
            log=output / "logs/extension-build.log",
        )
        after = source_tree_fingerprint(source_root)
        if after != source:
            raise IntegrityError("extension sources changed during the build")
        current = artifact_files(artifact)
        write_json(stamp, {"source": source, "artifacts": current})
    manifest = read_json(artifact / "manifest.json") or {}
    if (
        manifest.get("manifest_version") != 3
        or manifest.get("name") != "Sniptale"
        or not isinstance(manifest.get("background"), dict)
        or not isinstance(manifest["background"].get("service_worker"), str)
        or not isinstance(manifest.get("version"), str)
    ):
        raise IntegrityError("expected the real Sniptale MV3 extension artifact")
    for key in ("permissions", "optional_permissions"):
        if key in manifest and (
            not isinstance(manifest[key], list) or not all(isinstance(p, str) for p in manifest[key])
        ):
            raise IntegrityError("invalid extension permission inventory")
    probe = json.loads((paths.root / "tooling/desktop/e2e_lab/guest/browser-extension/manifest.json").read_text())
    # Product runtime bytes remain unchanged. These explicit fixture overrides select the dev
    # native-host channel, grant its optional permission, and use the allowlisted lab identity.
    manifest["key"] = probe["key"]
    manifest["version_name"] = manifest["version"] + " dev laboratory"
    manifest["permissions"] = sorted(set(manifest.get("permissions", [])) | {"nativeMessaging"})
    manifest["optional_permissions"] = [p for p in manifest.get("optional_permissions", []) if p != "nativeMessaging"]
    entries = []
    target = output / "real-extension.zip"
    with zipfile.ZipFile(target, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for entry in current:
            relative = entry["name"]
            file = artifact / relative
            entries.append(entry)
            if relative == "manifest.json":
                archive.writestr(relative, json.dumps(manifest, separators=(",", ":")))
            else:
                archive.write(file, relative)
    if artifact_files(artifact) != current:
        raise IntegrityError("extension artifact changed while packaging")
    return target, {
        "source": source,
        "version": manifest["version"],
        "original_manifest_sha256": sha256_file(artifact / "manifest.json"),
        "fixture_overrides": ["key", "version_name", "permissions", "optional_permissions"],
        "original_artifacts": entries,
        "bundle_sha256": sha256_file(target),
    }
