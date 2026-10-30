"""Official, age-gated offline inputs for the guest Playwright client."""
from __future__ import annotations

import base64
import hashlib
import json
import re


def urls(versions):
    selected = versions['browserAutomation']
    if set(selected) != {'node', 'playwrightCore'} or any(not isinstance(v, str) or re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+', v) is None for v in selected.values()):
        raise ValueError('browser automation versions must be exact')
    node, playwright = selected['node'], selected['playwrightCore']
    return {'node': f'https://nodejs.org/dist/v{node}/win-x64/node.exe',
            'playwrightCore': f'https://registry.npmjs.org/playwright-core/-/playwright-core-{playwright}.tgz'}


def resolve(versions, fetch, release, age):
    targets = urls(versions)
    selected = versions['browserAutomation']
    node = selected['node']
    records = json.loads(fetch('https://nodejs.org/dist/index.json'))
    record = next(item for item in records if item['version'] == 'v' + node)
    if not record.get('lts'): raise ValueError('guest Node must be an LTS release')
    # A date-only release record is admitted conservatively at the end of that UTC day.
    node_date = record['date'] + 'T23:59:59Z'
    sums = fetch(f'https://nodejs.org/dist/v{node}/SHASUMS256.txt').decode()
    matches = re.findall(r'^([0-9a-f]{64})\s+win-x64/node.exe$', sums, re.M)
    if len(matches) != 1: raise ValueError('official Windows Node checksum missing')
    playwright = selected['playwrightCore']
    metadata = json.loads(fetch(f'https://registry.npmjs.org/playwright-core/{playwright}'))
    if metadata.get('name') != 'playwright-core' or metadata.get('version') != playwright or metadata.get('dependencies') or metadata['dist']['tarball'] != targets['playwrightCore']:
        raise ValueError('unexpected Playwright package metadata')
    body = fetch(targets['playwrightCore'], 16 * 1024 * 1024)
    integrity = 'sha512-' + base64.b64encode(hashlib.sha512(body).digest()).decode()
    if metadata['dist']['integrity'] != integrity: raise ValueError('Playwright registry integrity mismatch')
    published = release('microsoft/playwright', 'v' + playwright)
    result = {'node': {'version': node, 'url': targets['node'], 'sha256': matches[0], 'publishedAt': node_date,
                       'source': 'https://nodejs.org/dist/index.json'},
              'playwrightCore': {'version': playwright, 'url': targets['playwrightCore'], 'sha256': hashlib.sha256(body).hexdigest(), **published}}
    check(versions, result, age)
    return result


def check(versions, entries, age):
    targets = urls(versions)
    if not isinstance(entries, dict) or set(entries) != set(targets): raise ValueError('browser tool inventory missing')
    for name, url in targets.items():
        item = entries[name]
        if item.get('url') != url or item.get('version') != versions['browserAutomation'][name] or not isinstance(item.get('sha256'), str) or re.fullmatch(r'[0-9a-f]{64}', item['sha256']) is None:
            raise ValueError('browser tool provenance differs from policy')
        expected = 'https://nodejs.org/dist/index.json' if name == 'node' else 'https://github.com/microsoft/playwright/releases/tag/v' + item['version']
        if item.get('source') != expected: raise ValueError('browser release source differs from policy')
        age(item.get('publishedAt'), versions['updatePolicy'])
