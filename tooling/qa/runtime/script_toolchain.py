"""Resolve exact script-tool releases from their package publishers."""
from __future__ import annotations

import json
from typing import Any


def resolve(versions: dict[str, Any], fetch, github_release, check_age) -> dict[str, Any]:
    result = {}
    for name, spec in versions.get("scriptTools", {}).items():
        version = spec["version"]
        manager = spec["manager"]
        if manager == "github":
            metadata = github_release(spec["repository"], spec["tag"])
        elif manager == "pypi":
            source = f"https://pypi.org/pypi/{spec['package']}/{version}/json"
            value = json.loads(fetch(source, 8 * 1024 * 1024))
            if value["info"]["version"] != version or not value["urls"] or any(item["yanked"] for item in value["urls"]):
                raise ValueError("script tool release identity mismatch or yanked release")
            metadata = {"source": source, "publishedAt": min(item["upload_time_iso_8601"] for item in value["urls"])}
        elif manager == "npm" and name == "renovate":
            metadata = github_release("renovatebot/renovate", version)
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
        result[name] = {**spec, **metadata}
    return result


def check(versions: dict[str, Any], locked: dict[str, Any], check_age) -> None:
    specs = versions.get("scriptTools", {})
    if not isinstance(locked, dict) or set(locked) != set(specs):
        raise ValueError("script tool inventory is stale")
    for name, spec in specs.items():
        item = locked[name]
        if {key: item.get(key) for key in spec} != spec or set(item) != set(spec) | {"source", "publishedAt"}:
            raise ValueError("script tool lock is stale")
        check_age(item["publishedAt"], versions["updatePolicy"], versions.get("securityExceptions", {}).get(name))
