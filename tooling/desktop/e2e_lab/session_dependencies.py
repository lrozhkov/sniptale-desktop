"""Hash-bound offline Windows installer and SDK inputs from the canonical lock."""
from __future__ import annotations

import json
import re
from errors import IntegrityError
from ssh import powershell_encoded
from system import download_https, require_sha256


def upload_wix_ui(paths, ssh, output):
    # Keep the controller API stable while all Windows packages share one setup path.
    lock = json.loads((paths.root / 'tooling/configs/qa/toolchain-lock.json').read_text())
    versions = json.loads((paths.root / 'tooling/configs/qa/tool-versions.json').read_text())
    cache = paths.scoped(paths.cache / 'session-dependencies/windows-packages')
    cache.mkdir(parents=True, exist_ok=True)
    remote = r'C:/SniptaleLab/build-tools/packages'
    transfers = []
    for name, item in lock['nuget'].items():
        if name not in {'wix', 'wixtoolset.ui.wixext', 'microsoft.windows.sdk.cpp', 'microsoft.windows.sdk.cpp.x64'}:
            raise IntegrityError('unexpected Windows tool package')
        version = versions['windows']['sdkPackage' if name.startswith('microsoft.windows.sdk.') else 'wix']
        if not isinstance(version, str) or re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+(?:\.[0-9]+)?', version) is None:
            raise IntegrityError('invalid Windows tool version')
        url = f'https://api.nuget.org/v3-flatcontainer/{name}/{version}/{name}.{version}.nupkg'
        if item['version'] != version or item['url'] != url or not isinstance(item['sha256'], str) or re.fullmatch(r'[0-9a-f]{64}', item['sha256']) is None:
            raise IntegrityError('Windows tool package differs from policy')
        package = cache / f'{name}.{version}.nupkg'
        if package.is_symlink():
            raise IntegrityError('Windows package cache redirected')
        if not package.exists():
            download_https(url, package, item['sha256'], timeout=300, max_bytes=256 * 1024 * 1024)
        require_sha256(package, item['sha256'])
        target = f'{remote}/{package.name}'
        transfers.append((target, item['sha256'], package))
    python = lock['python']['windows']
    version = versions['python']['version']
    if not isinstance(version, str) or re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+', version) is None:
        raise IntegrityError('invalid Python version')
    expected_url = f'https://www.python.org/ftp/python/{version}/python-{version}-embed-amd64.zip'
    if python['url'] != expected_url or python['sha256'] != versions['python']['windowsEmbeddableSha256'] or re.fullmatch(r'[0-9a-f]{64}', python['sha256']) is None:
        raise IntegrityError('Windows Python package differs from policy')
    package = cache / f'python-{version}-embed-amd64.zip'
    if package.is_symlink():
        raise IntegrityError('Windows Python cache redirected')
    if not package.exists():
        download_https(expected_url, package, python['sha256'], timeout=120, max_bytes=32 * 1024 * 1024)
    require_sha256(package, python['sha256'])
    transfers.append((f'{remote}/{package.name}', python['sha256'], package))
    expressions = ["if(Test-Path -LiteralPath '" + target + "'){(Get-FileHash -LiteralPath '" + target + "').Hash.ToLowerInvariant()}else{'missing'}" for target, expected, package in transfers]
    output = ssh.command(powershell_encoded("$ErrorActionPreference='Stop'; $ProgressPreference='SilentlyContinue'; New-Item -ItemType Directory -Force '" + remote + "' | Out-Null; " + '; '.join(expressions)), timeout=60).stdout.splitlines()
    if len(output) != len(transfers):
        raise IntegrityError('guest Windows tool inventory incomplete')
    for actual, (target, expected, package) in zip(output, transfers):
        if actual.strip() != expected:
            ssh.upload(package, target, timeout=600)
    return lock['nuget']['wixtoolset.ui.wixext']['dll_sha256']
