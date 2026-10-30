"""Prepare a hash-bound offline Playwright client in the isolated Windows guest."""

from __future__ import annotations

import json
import hashlib
import sys
import re
import tarfile
from pathlib import PurePosixPath

from errors import IntegrityError
from ssh import powershell_encoded
from system import download_https, require_sha256


def prepare(paths, ssh):
    if str(paths.root) not in sys.path:
        sys.path.insert(0, str(paths.root))
    from tooling.qa.runtime.browser_toolchain import check
    from tooling.qa.runtime.toolchain import check_age

    versions = json.loads((paths.root / "tooling/configs/qa/tool-versions.json").read_text())
    lock = json.loads((paths.root / "tooling/configs/qa/toolchain-lock.json").read_text())["browserAutomation"]
    check(versions, lock, check_age)
    cache = paths.scoped(paths.cache / "session-dependencies/browser-automation")
    cache.mkdir(parents=True, exist_ok=True)
    remote = "C:/SniptaleLab/browser-tools"
    packages = {}
    for name, filename in (("node", "node.exe"), ("playwrightCore", "playwright-core.tgz")):
        item = lock[name]
        package = cache / (item["sha256"] + "-" + filename)
        if package.is_symlink():
            raise IntegrityError("browser tool cache redirected")
        if not package.exists():
            download_https(item["url"], package, item["sha256"], timeout=180, max_bytes=128 * 1024 * 1024)
        require_sha256(package, item["sha256"])
        packages[filename] = package
    runtime = runtime_inventory(packages["playwright-core.tgz"])
    manifest = cache / "runtime-integrity.json"
    manifest.write_text(json.dumps(runtime, sort_keys=True, separators=(",", ":")))
    manifest_hash = hashlib.sha256(manifest.read_bytes()).hexdigest()
    expected = lock["node"]["sha256"] + ":" + lock["playwrightCore"]["sha256"]
    ready = ssh.command(
        powershell_encoded(
            "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; "
            f"if(Test-Path -LiteralPath '{remote}/ready.txt'){{Get-Content -Raw -LiteralPath '{remote}/ready.txt'}}else{{'missing'}}; exit 0"
        ),
        timeout=30,
    ).stdout.strip()
    verified = ssh.command(
        powershell_encoded(
            "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; "
            f"if((Test-Path '{remote}/node.exe') -and (Test-Path '{remote}/runtime-integrity.json')){{if((Get-FileHash '{remote}/node.exe').Hash.ToLowerInvariant() -eq '{lock['node']['sha256']}' -and (Get-FileHash '{remote}/runtime-integrity.json').Hash.ToLowerInvariant() -eq '{manifest_hash}'){{'verified'}}else{{'missing'}}}}else{{'missing'}}; exit 0"
        ),
        timeout=30,
    ).stdout.strip()
    if ready == expected and verified == "verified":
        return {name: {"version": item["version"], "sha256": item["sha256"]} for name, item in lock.items()}
    ssh.command(
        powershell_encoded(
            "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; "
            f"New-Item -ItemType Directory -Force '{remote}' | Out-Null; if(Test-Path '{remote}/ready.txt'){{Remove-Item '{remote}/ready.txt' -Force -ErrorAction Stop}}; exit 0"
        ),
        timeout=30,
    )
    packages["runtime-integrity.json"] = manifest
    for filename, package in packages.items():
        ssh.upload(package, remote + "/" + filename, timeout=180)
    script = "$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; "
    for name, filename in (("node", "node.exe"), ("playwrightCore", "playwright-core.tgz")):
        script += f"if((Get-FileHash '{remote}/{filename}').Hash.ToLowerInvariant() -ne '{lock[name]['sha256']}'){{throw 'browser tool hash mismatch'}}; "
    script += (
        f"Remove-Item '{remote}/package' -Recurse -Force -ErrorAction SilentlyContinue; "
        f"& tar.exe -xzf '{remote}/playwright-core.tgz' -C '{remote}'; if($LASTEXITCODE -ne 0){{throw 'Playwright extraction failed'}}; "
        f"if((& '{remote}/node.exe' --version).Trim() -ne 'v{lock['node']['version']}'){{throw 'Node version mismatch'}}; "
        f"$version=& '{remote}/node.exe' -p \"require('{remote}/package/package.json').version\"; "
        f"if($LASTEXITCODE -ne 0 -or $version.Trim() -ne '{lock['playwrightCore']['version']}'){{throw 'Playwright version mismatch'}}; "
        f"[IO.File]::WriteAllText('{remote}/ready.txt','{expected}'); Write-Output 'browser-tools-ready'"
    )
    ssh.command(powershell_encoded(script), timeout=120)
    return {name: {"version": item["version"], "sha256": item["sha256"]} for name, item in lock.items()}


def runtime_inventory(package):
    # Validate every tar member before Windows tar can materialize it. The npm package has no dependencies.
    with tarfile.open(package, "r:gz") as archive:
        count = total = 0
        names = set()
        runtime = {}
        for member in archive:
            count += 1
            path = PurePosixPath(member.name)
            total += member.size
            if (
                count > 5000
                or total > 128 * 1024 * 1024
                or not path.parts
                or path.parts[0] != "package"
                or str(path) != member.name.rstrip("/")
                or any(re.fullmatch(r"(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\..*)?", part, re.I) for part in path.parts)
                or path.is_absolute()
                or ".." in path.parts
                or "\\" in member.name
                or ":" in member.name
                or any(part.endswith((".", " ")) for part in path.parts)
                or member.name.casefold() in names
                or not (member.isfile() or member.isdir())
            ):
                raise IntegrityError("unsafe Playwright package inventory")
            names.add(member.name.casefold())
            if member.isfile():
                runtime[member.name] = hashlib.sha256(archive.extractfile(member).read()).hexdigest()
    return runtime
