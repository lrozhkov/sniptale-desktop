"""Resolve exact script-tool releases and verified data assets from publishers."""

from __future__ import annotations

import base64
import hashlib
import io
import json
import re
import tarfile
from typing import Any

ASSETS = {
    "shellcheck": ("koalaman/shellcheck", "v", "shellcheck-v{version}.linux.x86_64.tar.xz"),
    "shfmt": ("mvdan/sh", "v", "shfmt_v{version}_linux_amd64"),
    "actionlint": ("rhysd/actionlint", "v", "actionlint_{version}_linux_amd64.tar.gz"),
    "taplo": ("tamasfe/taplo", "", "taplo-linux-x86_64.gz"),
    "pwsh": ("PowerShell/PowerShell", "v", "powershell-{version}-linux-x64.tar.gz"),
    "PSScriptAnalyzer": ("PowerShell/PSScriptAnalyzer", "", "PSScriptAnalyzer.{version}.nupkg"),
}
HASH = re.compile(r"[0-9a-f]{64}")


def asset_identity(name: str, spec: dict[str, Any]) -> tuple[str, str]:
    repo, prefix, template = ASSETS[name]
    if spec["repository"] != repo:
        raise ValueError("script tool publisher differs from the approved owner")
    tag = prefix + spec["version"]
    return tag, f"https://github.com/{repo}/releases/download/{tag}/" + template.format(version=spec["version"])


def schema_files(body: bytes) -> dict[str, bytes]:
    result = {}
    maximum = 0
    with tarfile.open(fileobj=io.BytesIO(body), mode="r|gz") as archive:
        for count, member in enumerate(archive, 1):
            maximum += member.size
            if count > 30000 or maximum > 512 * 1024 * 1024:
                raise ValueError("schema distribution extraction exceeds budget")
            if member.name not in {
                "package/renovate-schema.json",
                "package/LICENSE",
                "package/LICENSE.md",
                "package/license",
            }:
                continue
            if not member.isfile() or member.size > 4 * 1024 * 1024:
                raise ValueError("schema distribution member is invalid")
            stream = archive.extractfile(member)
            if stream is None:
                raise ValueError("schema distribution member is unavailable")
            with stream:
                result[member.name.removeprefix("package/")] = stream.read(4 * 1024 * 1024 + 1)
    if "renovate-schema.json" not in result or not ({"LICENSE", "LICENSE.md", "license"} & set(result)):
        raise ValueError("schema distribution lacks schema or upstream license")
    return result


def schema_asset(version: str, previous: dict[str, Any], fetch) -> dict[str, Any]:
    url = f"https://registry.npmjs.org/renovate/-/renovate-{version}.tgz"
    old = previous.get("schemaAsset", {}) if previous.get("version") == version else {}
    if old.get("url") == url and HASH.fullmatch(old.get("sha256", "")) and HASH.fullmatch(old.get("schemaSha256", "")):
        return old
    metadata = json.loads(fetch(f"https://registry.npmjs.org/renovate/{version}", 8 * 1024 * 1024))
    if metadata.get("name") != "renovate" or metadata.get("version") != version or metadata["dist"]["tarball"] != url:
        raise ValueError("Renovate schema publisher identity mismatch")
    body = fetch(url, 64 * 1024 * 1024)
    integrity = "sha512-" + base64.b64encode(hashlib.sha512(body).digest()).decode()
    if integrity != metadata["dist"]["integrity"]:
        raise ValueError("Renovate schema registry integrity mismatch")
    files = schema_files(body)
    if json.loads(files["renovate-schema.json"]).get("x-renovate-version") != version:
        raise ValueError("Renovate schema version differs from its release")
    return {
        "url": url,
        "sha256": hashlib.sha256(body).hexdigest(),
        "schemaSha256": hashlib.sha256(files["renovate-schema.json"]).hexdigest(),
    }


def resolve(
    versions: dict[str, Any], fetch, github_release, check_age, previous: dict[str, Any] | None = None
) -> dict[str, Any]:
    result = {}
    old_tools = (previous or {}).get("scriptTools", {})
    for name, spec in versions.get("scriptTools", {}).items():
        version, manager = spec["version"], spec["manager"]
        extra = {}
        if manager == "github":
            tag, url = asset_identity(name, spec)
            metadata = github_release(spec["repository"], tag)
            old = old_tools.get(name, {})
            asset = old.get("linuxX64", {}) if old.get("version") == version else {}
            if asset.get("url") != url or HASH.fullmatch(asset.get("sha256", "")) is None:
                release = json.loads(fetch(f"https://api.github.com/repos/{spec['repository']}/releases/tags/{tag}"))
                published = next(item for item in release["assets"] if item["browser_download_url"] == url)
                digest = published.get("digest") or ""
                checksum = (
                    digest.removeprefix("sha256:")
                    if digest.startswith("sha256:")
                    else hashlib.sha256(fetch(url, 180 * 1024 * 1024)).hexdigest()
                )
                if HASH.fullmatch(checksum) is None:
                    raise ValueError("script distribution has no valid checksum")
                asset = {"url": url, "sha256": checksum}
            extra = {"tag": tag, "linuxX64": asset}
        elif manager == "pypi":
            source = f"https://pypi.org/pypi/{spec['package']}/{version}/json"
            value = json.loads(fetch(source, 8 * 1024 * 1024))
            if (
                value["info"]["version"] != version
                or not value["urls"]
                or any(item["yanked"] for item in value["urls"])
            ):
                raise ValueError("script tool release identity mismatch or yanked release")
            metadata = {"source": source, "publishedAt": min(item["upload_time_iso_8601"] for item in value["urls"])}
        elif manager == "npm" and name in {"renovate", "typescript"}:
            repo, tag = (
                ("renovatebot/renovate", version) if name == "renovate" else ("microsoft/TypeScript", "v" + version)
            )
            metadata = github_release(repo, tag)
            if name == "renovate":
                if spec.get("usage") != "configuration-schema-only":
                    raise ValueError("local Renovate execution is not an approved checking dependency")
                extra = {"schemaAsset": schema_asset(version, old_tools.get(name, {}), fetch)}
        elif manager == "npm":
            source = f"https://registry.npmjs.org/{spec['package']}"
            value = json.loads(fetch(source, 128 * 1024 * 1024))
            if value["versions"][version].get("deprecated"):
                raise ValueError("deprecated script tool release")
            metadata = {"source": source + "/" + version, "publishedAt": value["time"][version]}
        else:
            raise ValueError("unsupported script tool publisher")
        exception = versions.get("securityExceptions", {}).get(name)
        if exception is not None and exception.get("fixedVersion") != version:
            raise ValueError("security exception identifies another version")
        check_age(metadata["publishedAt"], versions["updatePolicy"], exception)
        result[name] = {**spec, **metadata, **extra}
    return result


def check(versions: dict[str, Any], locked: dict[str, Any], check_age) -> None:
    specs = versions.get("scriptTools", {})
    if not isinstance(locked, dict) or set(locked) != set(specs):
        raise ValueError("script tool inventory is stale")
    for name, spec in specs.items():
        version, manager = spec["version"], spec["manager"]
        if manager not in {"github", "pypi", "npm"}:
            raise ValueError("unsupported script tool publisher")
        expected_keys = {"version", "manager", "repository" if manager == "github" else "package"}
        if name == "renovate":
            expected_keys.add("usage")
        if (
            set(spec) != expected_keys
            or not isinstance(version, str)
            or re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version) is None
        ):
            raise ValueError("script tool catalog requires exact minimal publisher/version fields")
        item = locked[name]
        extra = {"tag", "linuxX64"} if manager == "github" else {"schemaAsset"} if name == "renovate" else set()
        if {key: item.get(key) for key in spec} != spec or set(item) != set(spec) | {"source", "publishedAt"} | extra:
            raise ValueError("script tool lock is stale")
        if manager == "github":
            tag, url = asset_identity(name, spec)
            if (
                item["tag"] != tag
                or item["linuxX64"].get("url") != url
                or HASH.fullmatch(item["linuxX64"].get("sha256", "")) is None
            ):
                raise ValueError("script tool asset identity is stale")
        if name == "renovate":
            asset = item["schemaAsset"]
            if (
                spec.get("usage") != "configuration-schema-only"
                or asset.get("url") != f"https://registry.npmjs.org/renovate/-/renovate-{version}.tgz"
                or HASH.fullmatch(asset.get("sha256", "")) is None
                or HASH.fullmatch(asset.get("schemaSha256", "")) is None
            ):
                raise ValueError("Renovate schema asset identity is stale")
        check_age(item["publishedAt"], versions["updatePolicy"], versions.get("securityExceptions", {}).get(name))
