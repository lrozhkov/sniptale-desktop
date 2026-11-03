import assert from 'node:assert/strict';
import test from 'node:test';
import { parsePromptView } from '../src/client/prompts.ts';
await test('prompt parsing rejects unknown actions, excess destinations and untrusted labels', () => {
  const valid = {generation:1, content:{kind:'connection', extension:'Extension', launcher:'Browser', category:'browser'}};
  assert.deepEqual(parsePromptView(valid), valid);
  assert.deepEqual(parsePromptView({generation:2, content:{kind:'quit-recording'}}), {generation:2, content:{kind:'quit-recording'}});
  for (const raw of [null, {}, {...valid, generation:0}, {...valid, generation:Infinity},
    {...valid, content:{...valid.content, command:'capture'}},
    {...valid, content:{...valid.content, launcher:'raw\nvalue'}},
    {...valid, content:{kind:'destination', localSaved:false, targets:Array(17).fill('Browser')}},
    {...valid, content:{kind:'destination', localSaved:false, targets:[null]}},
    {...valid, content:{kind:'unknown'}}]) assert.throws(() => parsePromptView(raw));
});
