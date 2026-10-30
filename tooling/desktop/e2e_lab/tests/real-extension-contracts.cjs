const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {webcrypto} = require('node:crypto');
const source = fs.readFileSync(process.argv[2], 'utf8');
async function check(reply) {
  const nonces = new Set();
  let calls = 0;
  const result = await vm.runInNewContext(source, {
    __labAction: 'connect', crypto: webcrypto, navigator: {userAgent:'Chrome/1.0'},
    setTimeout: callback => callback(),
    chrome: {runtime: {sendMessage: async message => {
      const freshness = message.__sniptaleRuntimeFreshness;
      assert.ok(freshness && Number.isSafeInteger(freshness.issuedAtEpochMs));
      assert.ok(Math.abs(Date.now() - freshness.issuedAtEpochMs) < 5000);
      assert.match(freshness.nonce, /^[0-9a-f-]{36}$/);
      assert.ok(!nonces.has(freshness.nonce));
      nonces.add(freshness.nonce);
      assert.match(message.requestId, /^[0-9a-f-]{36}$/);
      return reply(message, ++calls);
    }}}
  });
  return {result, calls};
}
(async () => {
  const connected = {success:true, status:{connectionState:'connected', controllerLease:{}, settingsRevision:'revision-1', effectiveSettings:{screenshots:{includeCursor:true}}}};
  const valid = await check(() => connected);
  assert.equal(valid.result.passed, true);
  assert.equal(valid.calls, 2);
  const pending = await check((message, calls) => calls < 4 ? {...connected, status:{...connected.status, effectiveSettings:null}} : connected);
  assert.equal(pending.result.passed, true);
  assert.equal(pending.calls, 4);
  const mutation = await check(() => ({success:false}));
  assert.equal(mutation.result.category, 'runtime-mutation-rejected');
  assert.equal(mutation.calls, 1);
  const query = await check(message => message.type === 'NATIVE_APP_MUTATION' ? connected : {success:false});
  assert.equal(query.result.category, 'runtime-query-rejected');
  for (const patch of [{controllerLease:null}, {settingsRevision:null}, {effectiveSettings:null}, {connectionState:'not-connected'}, {connectionState:'repair-required'}, {connectionState:'error'}]) {
    const denied = await check(() => ({...connected, status:{...connected.status, ...patch}}));
    assert.equal(denied.result.passed, false);
  }
  process.stdout.write('OK real extension request freshness, rejection, lease, and settings contracts\n');
})().catch(error => { process.stderr.write(error.stack + '\n'); process.exitCode = 1; });
