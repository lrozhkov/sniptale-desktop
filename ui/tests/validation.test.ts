import assert from 'node:assert/strict';
import test from 'node:test';
import { parseCaptureSnapshot } from '../src/client/validation.ts';

const supported = { supported: true, permission: 'prompt', sourceAvailable: true };
const snapshot = () => ({ revision: 1, operationEpoch: 0, defaultQuality: 'balanced', setupRequired:false, phase: 'idle', kind: 'screenshot', elapsedMs: 0, commands: ['start'], outcome: null,
  capabilities: { screenshot: { ...supported }, video: { ...supported }, targets: {
    region: { ...supported }, display: { ...supported }, window: { ...supported },
  } } });

await test('capability support, permission, and source availability remain independent', () => {
  const raw = snapshot(); raw.capabilities.video.supported = false;
  raw.capabilities.targets.window.sourceAvailable = false;
  const parsed = parseCaptureSnapshot(raw);
  assert.equal(parsed.capabilities.screenshot.permission, 'prompt');
  assert.equal(parsed.capabilities.video.supported, false);
  assert.equal(parsed.capabilities.targets.window.sourceAvailable, false);
  raw.capabilities.video.supported = true;
  assert.equal(parsed.capabilities.video.supported, false);
});

await test('rejects foreign payloads, invalid numbers, unknown commands, and incomplete capabilities', () => {
  const bad = [null, [], {}, {...snapshot(), setupRequired:'no'}, { ...snapshot(), operationEpoch: -1 }, { ...snapshot(), operationEpoch: Number.MAX_SAFE_INTEGER + 1 }, { ...snapshot(), revision: -1 }, { ...snapshot(), elapsedMs: Infinity },
    { ...snapshot(), revision: Number.MAX_SAFE_INTEGER + 1 }, { ...snapshot(), phase: 'publishing-secret-file' },
    { ...snapshot(), commands: ['start', 'start'] }, { ...snapshot(), commands: ['shell-execute'] },
    { ...snapshot(), media: 'unrequested bytes' }, { ...snapshot(), capabilities: {} },
    { ...snapshot(), capabilities: { ...snapshot().capabilities, screenshot: { supported: true } } }];
  for (const value of bad) assert.throws(() => parseCaptureSnapshot(value));
});
