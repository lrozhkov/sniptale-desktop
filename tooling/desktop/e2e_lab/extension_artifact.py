"""Package a real extension runtime with an explicit, isolated lab identity."""

from __future__ import annotations

import json
import re
import stat
import shutil
import tempfile
import zipfile
from pathlib import Path, PurePosixPath

from artifacts import source_tree_fingerprint
from errors import IntegrityError
from state import read_json, write_json
from system import run, sha256_file


def artifact_files(artifact, *, include_maps=False):
    entries = []
    total = 0
    names = set()
    for file in artifact.rglob("*"):
        mode = file.lstat().st_mode
        if stat.S_ISDIR(mode):
            continue
        if not stat.S_ISREG(mode):
            raise IntegrityError("extension artifact contains a link or special file")
        if file.suffix == ".map" and not include_maps:
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
    if artifact.suffix.lower() == ".zip":
        if not (source_root / ".git").exists():
            raise IntegrityError("extension source must be a Git checkout")
        source = source_tree_fingerprint(source_root)
        bundle_hash = sha256_file(artifact)
        with tempfile.TemporaryDirectory(prefix="release-extension-", dir=output) as temporary:
            directory = Path(temporary)
            extract_release(artifact, directory)
            current = artifact_files(directory, include_maps=True)
            target, metadata = package_runtime(
                paths, directory, current, source, output, include_maps=True, input_kind="release-zip"
            )
        if source_tree_fingerprint(source_root) != source or sha256_file(artifact) != bundle_hash:
            raise IntegrityError("extension release inputs changed while packaging")
        metadata["input_kind"] = "release-zip"
        metadata["input_bundle_sha256"] = bundle_hash
        return target, metadata
    if artifact != source_root / ".tmp/e2e-builds/test":
        raise IntegrityError("real extension proof requires the owner's test-e2e build output")
    if not (source_root / ".git").exists():
        raise IntegrityError("extension source must be a Git checkout")
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
    return package_runtime(paths, artifact, current, source, output)


def package_runtime(paths, artifact, current, source, output, *, include_maps=False, input_kind="test-e2e-build"):
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
    original_manifest = json.loads(json.dumps(manifest))
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
    if artifact_files(artifact, include_maps=include_maps) != current:
        raise IntegrityError("extension artifact changed while packaging")
    return target, {
        "source": source,
        "version": manifest["version"],
        "original_manifest_sha256": sha256_file(artifact / "manifest.json"),
        "fixture_overrides": {
            key: {"before": original_manifest.get(key), "after": manifest.get(key)}
            for key in ("key", "version_name", "permissions", "optional_permissions")
            if original_manifest.get(key) != manifest.get(key)
        },
        "proof_scope": input_kind
        + " runtime with laboratory identity and pregranted nativeMessaging; excludes pristine permission and store-identity acceptance",
        "original_artifacts": entries,
        "bundle_sha256": sha256_file(target),
    }


def extract_release(bundle: Path, destination: Path):
    if not bundle.is_file() or bundle.stat().st_size > 256 * 1024 * 1024:
        raise IntegrityError("extension release ZIP exceeds transfer budget")
    try:
        with zipfile.ZipFile(bundle) as archive:
            entries = archive.infolist()
            if len(entries) > 5000:
                raise IntegrityError("extension release ZIP exceeds file budget")
            names = set()
            prefixes = {}
            total = 0
            for entry in entries:
                name = entry.filename.rstrip("/") if entry.is_dir() else entry.filename
                relative = PurePosixPath(name)
                mode = entry.external_attr >> 16
                if (
                    not name
                    or entry.orig_filename != entry.filename
                    or relative.is_absolute()
                    or relative.as_posix() != name
                    or any(part in (".", "..") or part.endswith((".", " ")) for part in relative.parts)
                    or any(character in name for character in '\\:<>"|?*')
                    or any(ord(character) < 32 for character in name)
                    or name.casefold() in names
                    or any(
                        part.split(".")[0].upper()
                        in {
                            "CON",
                            "PRN",
                            "AUX",
                            "NUL",
                            *[f"COM{i}" for i in range(1, 10)],
                            *[f"LPT{i}" for i in range(1, 10)],
                        }
                        for part in relative.parts
                    )
                    or stat.S_IFMT(mode) not in (0, stat.S_IFREG, stat.S_IFDIR)
                    or entry.flag_bits & 1
                ):
                    raise IntegrityError("extension release ZIP contains an unsafe path or entry")
                for depth in range(1, len(relative.parts) + 1):
                    prefix = PurePosixPath(*relative.parts[:depth]).as_posix()
                    previous = prefixes.setdefault(prefix.casefold(), prefix)
                    if previous != prefix:
                        raise IntegrityError("extension release ZIP contains a Windows path alias")
                names.add(name.casefold())
                total += entry.file_size
                if entry.file_size > 64 * 1024 * 1024 or total > 256 * 1024 * 1024:
                    raise IntegrityError("extension release ZIP exceeds transfer budget")
            for entry in entries:
                target = destination / entry.filename
                if entry.is_dir():
                    target.mkdir(parents=True, exist_ok=True)
                    continue
                target.parent.mkdir(parents=True, exist_ok=True)
                with archive.open(entry) as reader, target.open("xb") as writer:
                    shutil.copyfileobj(reader, writer, length=64 * 1024)
    except (OSError, ValueError, zipfile.BadZipFile, RuntimeError) as exc:
        raise IntegrityError("extension release ZIP could not be verified") from exc
