"""Bounded, fresh advisory queries; an unavailable source is never a clean result."""

from __future__ import annotations

import json
import os
import re
import shutil
import subprocess
from functools import lru_cache
import time
import urllib.error
import urllib.parse
import urllib.request
from collections.abc import Callable
from typing import Any


class AdvisoryError(ValueError):
    """A source or response cannot support an audit decision."""


class ApprovedRedirects(urllib.request.HTTPRedirectHandler):
    """Reject a changed authority before urllib forwards any authorization header."""

    def __init__(self, source_url: str):
        self.source = urllib.parse.urlsplit(source_url)
        super().__init__()

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        target = urllib.parse.urlsplit(newurl)
        hosts = {self.source.hostname}
        if self.source.hostname == "github.com" and "/releases/download/" in self.source.path:
            hosts.update({"release-assets.githubusercontent.com", "objects.githubusercontent.com"})
        if target.scheme != "https" or target.hostname not in hosts or target.username or target.password:
            raise AdvisoryError("source redirect changed approved authority")
        return super().redirect_request(req, fp, code, msg, headers, newurl)


def unique_object(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise AdvisoryError("duplicate JSON key")
        result[key] = value
    return result


def decode(data: bytes) -> dict[str, Any]:
    if len(data) > 8 * 1024 * 1024:
        raise AdvisoryError("advisory input exceeds byte budget")
    try:
        value = json.loads(data, object_pairs_hook=unique_object)
    except (UnicodeError, json.JSONDecodeError, RecursionError) as exc:
        raise AdvisoryError("invalid advisory JSON") from exc
    if not isinstance(value, dict):
        raise AdvisoryError("advisory response must be an object")
    return value


@lru_cache(maxsize=1)
def github_token() -> str | None:
    """Use the supplied CI token or the already configured local GitHub CLI login."""
    if os.environ.get("GH_TOKEN"):
        return os.environ["GH_TOKEN"]
    executable = shutil.which("gh")
    if executable is None:
        return None
    result = subprocess.run([executable, "auth", "token"], capture_output=True, text=True, check=False, timeout=10)
    return result.stdout.strip() if result.returncode == 0 else None


def request_json(url: str, payload: dict[str, Any] | None = None) -> dict[str, Any]:
    parsed = urllib.parse.urlsplit(url)
    if (
        parsed.scheme != "https"
        or parsed.hostname
        not in {
            "api.osv.dev",
            "services.nvd.nist.gov",
            "api.github.com",
            "cveawg.mitre.org",
            "registry.npmjs.org",
        }
        or parsed.username
        or parsed.password
        or parsed.fragment
    ):
        raise AdvisoryError("unapproved advisory endpoint")
    body = json.dumps(payload).encode() if payload is not None else None
    headers = {
        "User-Agent": "SniptaleEnvironmentAudit/1",
        "Content-Type": "application/json",
    }
    token = github_token() if parsed.hostname == "api.github.com" else None
    if parsed.hostname == "api.github.com" and token:
        headers["Authorization"] = "Bearer " + token
    request = urllib.request.Request(url, data=body, headers=headers)
    opener = urllib.request.build_opener(ApprovedRedirects(url))
    for attempt in range(3):
        try:
            with opener.open(request, timeout=25) as response:
                if urllib.parse.urlsplit(response.geturl()).hostname != parsed.hostname:
                    raise AdvisoryError("advisory response changed authority")
                return decode(response.read(8 * 1024 * 1024 + 1))
        except urllib.error.HTTPError as exc:
            if exc.code not in {429, 500, 502, 503, 504} or attempt == 2:
                raise AdvisoryError(f"advisory HTTP source unavailable ({exc.code})") from None
            time.sleep(6 * (attempt + 1))
        except (urllib.error.URLError, TimeoutError, OSError):
            if attempt == 2:
                raise AdvisoryError("advisory transport unavailable (" + str(parsed.hostname) + ")") from None
            time.sleep(2 * (attempt + 1))
    raise AdvisoryError("advisory retry budget exhausted")


Fetch = Callable[[str, dict[str, Any] | None], dict[str, Any]]
IDENTIFIER = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]{0,127}")


def identifiers(value: Any) -> list[str]:
    if not isinstance(value, list) or len(value) > 4000:
        raise AdvisoryError("invalid advisory list")
    if any(not isinstance(item, str) or IDENTIFIER.fullmatch(item) is None for item in value):
        raise AdvisoryError("invalid advisory identifier")
    return sorted(set(value))


def osv_queries(queries: list[dict[str, Any]], fetch: Fetch = request_json) -> list[list[str]]:
    if not 1 <= len(queries) <= 100:
        raise AdvisoryError("invalid OSV query count")
    results: list[list[str]] = []
    for query in queries:
        # Independent bounded pages preserve exact query identity and reject incomplete results.
        found: list[str] = []
        current = dict(query)
        seen: set[str] = set()
        for _ in range(8):
            value = fetch("https://api.osv.dev/v1/query", current)
            if set(value) - {"vulns", "next_page_token"}:
                raise AdvisoryError("unexpected OSV response fields")
            records = value.get("vulns", [])
            if not isinstance(records, list) or not all(isinstance(item, dict) for item in records):
                raise AdvisoryError("invalid OSV vulnerability records")
            found.extend(identifiers([item.get("id") for item in records]))
            token = value.get("next_page_token")
            if token is None:
                results.append(identifiers(found))
                break
            if not isinstance(token, str) or not 0 < len(token) <= 4096 or token in seen:
                raise AdvisoryError("invalid OSV pagination")
            seen.add(token)
            current = {**query, "page_token": token}
        else:
            raise AdvisoryError("OSV pagination exceeds budget")
    return results


def cpython_query(version: str, fetch: Fetch = request_json) -> list[str]:
    if re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version) is None:
        raise AdvisoryError("CPython audit requires an exact stable release")
    reference = fetch("https://api.github.com/repos/python/cpython/git/ref/tags/v" + version, None)
    for _ in range(4):
        obj = reference.get("object")
        if (
            not isinstance(obj, dict)
            or not isinstance(obj.get("sha"), str)
            or re.fullmatch(r"[0-9a-f]{40}", obj["sha"]) is None
        ):
            raise AdvisoryError("invalid CPython release reference")
        if obj.get("type") == "commit":
            return osv_queries([{"commit": obj["sha"]}], fetch)[0]
        if obj.get("type") != "tag":
            raise AdvisoryError("CPython release reference is not a commit or tag")
        reference = fetch("https://api.github.com/repos/python/cpython/git/tags/" + obj["sha"], None)
    raise AdvisoryError("CPython release reference exceeds dereference budget")


def nvd_query(vendor: str, product: str, version: str, fetch: Fetch = request_json) -> list[str]:
    if any(re.fullmatch(r"[a-zA-Z0-9_.-]+", part) is None for part in (vendor, product, version)):
        raise AdvisoryError("invalid CPE identity")
    cpe = f"cpe:2.3:a:{vendor}:{product}:{version}:*:*:*:*:*:*:*"
    found: list[str] = []
    start = 0
    for _ in range(8):
        query = urllib.parse.urlencode(
            {"cpeName": cpe, "isVulnerable": "", "resultsPerPage": 2000, "startIndex": start}
        )
        value = fetch("https://services.nvd.nist.gov/rest/json/cves/2.0?" + query, None)
        total = value.get("totalResults")
        records = value.get("vulnerabilities")
        if (
            type(total) is not int
            or not 0 <= total <= 4000
            or value.get("startIndex") != start
            or not isinstance(records, list)
            or len(records) > 2000
        ):
            raise AdvisoryError("invalid NVD page")
        for record in records:
            if not isinstance(record, dict) or not isinstance(record.get("cve"), dict):
                raise AdvisoryError("invalid NVD record")
            cve = record["cve"]
            if cve.get("vulnStatus") != "Rejected":
                found.extend(identifiers([cve.get("id")]))
        start += len(records)
        if start == total:
            return identifiers(found)
        if not records or start > total:
            raise AdvisoryError("incomplete NVD pagination")
        time.sleep(6)
    raise AdvisoryError("NVD pagination exceeds budget")


def osv_batch(queries: list[dict[str, Any]], fetch: Fetch = request_json) -> list[list[str]]:
    """Query locked package closures; preserve order and drain any partial batch result."""
    if not queries or len(queries) > 2000:
        raise AdvisoryError("dependency advisory query budget exceeded")
    results: list[list[str]] = []
    for start in range(0, len(queries), 100):
        batch = queries[start : start + 100]
        value = fetch("https://api.osv.dev/v1/querybatch", {"queries": batch})
        rows = value.get("results")
        if set(value) != {"results"} or not isinstance(rows, list) or len(rows) != len(batch):
            raise AdvisoryError("incomplete dependency advisory batch")
        for query, row in zip(batch, rows, strict=True):
            if not isinstance(row, dict) or set(row) - {"vulns", "next_page_token"}:
                raise AdvisoryError("invalid dependency advisory result")
            records = row.get("vulns", [])
            if not isinstance(records, list) or any(not isinstance(item, dict) for item in records):
                raise AdvisoryError("invalid dependency advisory identifiers")
            found = identifiers([item.get("id") for item in records])
            if row.get("next_page_token"):
                found = identifiers(found + osv_queries([query], fetch)[0])
            results.append(found)
    return results
