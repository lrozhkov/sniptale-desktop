import assert from 'node:assert/strict';
import test from 'node:test';
import { parsePanelInsets } from '../src/client/panel-frame.ts';
await test('panel frame responses accept bounded insets and reject malformed layout data', () => {
  const valid = {left:12, top:332, feedbackAbove:true};
  assert.deepEqual(parsePanelInsets(valid), valid);
  for (const raw of [null, {}, {...valid, top:Infinity}, {...valid, left:-1}, {...valid, top:900},
    {...valid, feedbackAbove:'yes'}, {...valid, extra:true}]) assert.throws(() => parsePanelInsets(raw));
});
