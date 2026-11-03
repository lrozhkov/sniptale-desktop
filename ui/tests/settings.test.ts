import assert from 'node:assert/strict';
import test from 'node:test';
import { encodeSettingsRequest, parseSettingsSnapshot } from '../src/client/settings.ts';

const native = {
  profileRevision:3, selectedFolder:null, delivery:{targets:[], targetId:null, askAfterCapture:false}, microphones:[], audioSources:[{source:'system', available:true}],
  draft:{destinationPath:'/captures', screenshotIncludeCursor:true, videoQuality:'medium', videoFps:'fps60',
    videoBitrateOverrideMbps:null, videoHardwarePreference:'force-software', videoIncludeCursor:true,
    collectPointerEvents:false, videoMaxDurationSeconds:7200, audioSourceMode:'system', microphoneDeviceId:null,
    audioBitrateKbps:128, hotkeys:{'recording-stop':'Ctrl+Shift+X'}},
};
await test('settings transport preserves owner values and maps presentation names in both directions', () => {
  const parsed = parseSettingsSnapshot(native);
  assert.equal(parsed.draft.videoQuality, 'balanced');
  assert.equal(parsed.draft.videoFps, '60');
  assert.equal(parsed.draft.videoHardwarePreference, 'software');
  assert.equal(parsed.draft.microphoneDeviceId, '');
  assert.deepEqual(encodeSettingsRequest({command:'apply', expectedRevision:3, draft:parsed.draft}),
    {command:'apply', expectedRevision:3, draft:native.draft});
  assert.equal(parseSettingsSnapshot({...native, selectedFolder:'/chosen'}).selectedFolder, '/chosen');
});
await test('settings transport rejects malformed snapshots instead of guessing control values', () => {
  for (const raw of [null, {...native, extra:true}, {...native, profileRevision:-1},
    {...native, draft:{...native.draft, videoQuality:'balanced'}}, {...native, draft:{...native.draft, videoFps:'90'}},
    {...native, draft:{...native.draft, collectPointerEvents:'yes'}}, {...native, draft:{...native.draft, hotkeys:{stop:4}}},
    {...native, delivery:{targets:[], targetId:42, askAfterCapture:false}},
    {...native, microphones:[{id:null, name:'', available:true}]}, {...native, audioSources:[{source:'unknown', available:true}]},
  ]) assert.throws(() => parseSettingsSnapshot(raw));
});
