// Exercise callback rejection handling locally; no browser or Native Messaging host is started.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync('tooling/desktop/e2e_lab/guest/browser-extension/probe.js', 'utf8');
async function prove(trigger) {
  let timeout, message, disconnect, title, writes = 0, closed = false;
  const document = {};
  Object.defineProperty(document, 'title', {set(value) {
    if (++writes === 1) throw new Error('injected reporting failure');
    title = value;
  }});
  const port = {onMessage: {addListener(callback) {message = callback;}},
    onDisconnect: {addListener(callback) {disconnect = callback;}},
    postMessage() {}, disconnect() {closed = true;}};
  vm.runInNewContext(source, {document, navigator: {userAgent: 'Chrome/fixture'},
    chrome: {runtime: {id: 'fixture', connectNative() {return port;}}},
    setTimeout(callback) {timeout = callback; return 1;}, clearTimeout() {},
    fetch: async () => {assert.fail('reporting must fail before network access');}}, {timeout: 1000});
  if (trigger === 'timeout') timeout();
  else if (trigger === 'disconnect') disconnect();
  else message({type: 'app.operation.failed', code: 'fixture'});
  await new Promise(resolve => {setImmediate(resolve);});
  assert.equal(title, 'sniptale-e2e-report-failed');
  assert.equal(writes, 2);
  assert.equal(closed, trigger === 'message');
}
(async () => {
  for (const trigger of ['timeout', 'disconnect', 'message']) await prove(trigger);
  process.stdout.write('OK callback reporting rejection is handled and port cleanup runs\n');
})().catch(error => {process.stderr.write(error.stack + '\n'); process.exitCode = 1;});
