'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {createRequire} = require('node:module');
const root = path.resolve(__dirname, '../../..');
const load = createRequire(path.join(root, '.tmp/qa-tools/script-tools/npm/package.json'));
const jsonata = load('jsonata');
(async () => {
  const config = JSON.parse(fs.readFileSync(path.join(root, 'renovate.json'), 'utf8'));
  const transforms = config.customDatasources.cpython.transformTemplates;
  assert.equal(transforms.length, 1);
  const expression = jsonata(transforms[0]);
  const release = {name: 'Python 3.15.2', is_published: true, pre_release: false, release_date: '2026-10-27T12:00:00Z'};
  const result = await expression.evaluate([
    release,
    {...release, name: 'Python 3.16.0a1', pre_release: true},
    {...release, name: 'Python 3.15.3', is_published: false},
    {...release, name: 'unrelated package'},
  ]);
  assert.deepEqual(JSON.parse(JSON.stringify(result)), {releases: [{version: '3.15.2', releaseTimestamp: release.release_date}]});
  assert.deepEqual(JSON.parse(JSON.stringify(await expression.evaluate([]))), {releases: []});
  console.log('OK CPython update feed preserves authoritative timestamps and rejects unpublished/prerelease records');
})().catch(() => {
  console.error('FAIL CPython update feed contract');
  process.exitCode = 1;
});
