"""Apply the common release-age policy to the entire locked checker graph."""

from __future__ import annotations

import json
import re
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from typing import Any

from .tool_dependencies import locked_packages


def direct_tools(
    versions: dict[str, Any], tools: dict[str, Any]
) -> dict[tuple[str, str, str], tuple[dict[str, Any], Any]]:
    result = {}
    for name, spec in versions["scriptTools"].items():
        if spec["manager"] not in {"npm", "pypi"} or name == "renovate":
            continue
        package = spec["package"]
        ecosystem = "npm" if spec["manager"] == "npm" else "PyPI"
        if ecosystem == "PyPI":
            package = re.sub(r"[-_.]+", "-", package).lower()
        result[(ecosystem, package, spec["version"])] = (tools[name], versions.get("securityExceptions", {}).get(name))
    return result


def source_for(package: tuple[str, str, str]) -> str:
    eco, name, version = package
    return f"https://pypi.org/pypi/{name}/{version}/json" if eco == "PyPI" else "https://registry.npmjs.org/" + name


def resolve(
    root: Path, versions: dict[str, Any], tools: dict[str, Any], previous: list[dict[str, Any]], fetch, check_age
) -> list[dict[str, Any]]:
    direct = direct_tools(versions, tools)
    old = {(item["ecosystem"], item["name"], item["version"]): item for item in previous}

    def inspect(package: tuple[str, str, str]) -> dict[str, Any]:
        eco, name, version = package
        if package in direct:
            metadata, exception = direct[package]
            source, publication = metadata["source"], metadata["publishedAt"]
        else:
            exception = None
            source = source_for(package)
            cached = old.get(package, {})
            if cached.get("source") == source:
                publication = cached["publishedAt"]
            else:
                value = json.loads(fetch(source, 128 * 1024 * 1024))
                if eco == "PyPI":
                    if (
                        value["info"]["version"] != version
                        or not value["urls"]
                        or any(item["yanked"] for item in value["urls"])
                    ):
                        raise ValueError("checker dependency is unavailable or yanked")
                    publication = min(item["upload_time_iso_8601"] for item in value["urls"])
                else:
                    if value["versions"][version].get("deprecated"):
                        raise ValueError("checker dependency is deprecated")
                    publication = value["time"][version]
        try:
            check_age(publication, versions["updatePolicy"], exception)
        except (RuntimeError, ValueError) as exc:
            raise ValueError(f"checker dependency {eco}:{name}@{version} published {publication}: {exc}") from exc
        return {"ecosystem": eco, "name": name, "version": version, "source": source, "publishedAt": publication}

    with ThreadPoolExecutor(max_workers=4) as pool:
        pending = [pool.submit(inspect, package) for package in locked_packages(root)]
        result, errors = [], []
        for task in pending:
            try:
                result.append(task.result())
            except (OSError, RuntimeError, ValueError) as exc:
                errors.append(str(exc))
    if errors:
        raise ValueError("checker publication admission failed: " + "; ".join(errors[:20]))
    return result


def check(
    root: Path, versions: dict[str, Any], tools: dict[str, Any], records: list[dict[str, Any]], check_age
) -> None:
    direct = direct_tools(versions, tools)
    expected = set(locked_packages(root))
    seen = set()
    for item in records:
        if set(item) != {"ecosystem", "name", "version", "source", "publishedAt"}:
            raise ValueError("invalid checker dependency publication record")
        package = (item["ecosystem"], item["name"], item["version"])
        metadata, exception = direct.get(package, ({"source": source_for(package)}, None))
        if package not in expected or package in seen or item["source"] != metadata["source"]:
            raise ValueError("checker dependency publication inventory is stale")
        seen.add(package)
        check_age(item["publishedAt"], versions["updatePolicy"], exception)
    if seen != expected:
        raise ValueError("every locked checker dependency requires publication-age evidence")
