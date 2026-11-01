'use strict';
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const source = fs.readFileSync(process.argv[2], 'utf8');
const fixtures = JSON.parse(fs.readFileSync(path.resolve(__dirname, '../../../../contracts/native/fixtures/positive/contract-fixtures.json'), 'utf8'));
const copy = value => JSON.parse(JSON.stringify(value));
function start() {
  const messages = [], reports = [];
  let message, disconnect, timeout;
  const port = {
    postMessage: value => messages.push(copy(value)),
    disconnect: () => disconnect(),
    onMessage: {addListener: handler => { message = handler; }},
    onDisconnect: {addListener: handler => { disconnect = handler; }},
  };
  vm.runInNewContext(source, {
    chrome: {runtime: {id: 'elhndangpenapadjimfoodlaenpcndpo', connectNative: name => {
      assert.equal(name, 'com.sniptale.dev.native_host'); return port;
    }}},
    navigator: {userAgent: 'Chrome/156.0.0.0'}, document: {}, Date,
    setTimeout: handler => { timeout = handler; return 1; }, clearTimeout: () => {},
    fetch: async (_url, request) => { reports.push(JSON.parse(request.body)); },
  });
  return {messages, reports, message: value => message(copy(value)), timeout: () => timeout()};
}
async function run() {
  for (const selectionStatus of ['selected', 'available', 'selection-required']) {
    const runtime = start();
    const hello = runtime.messages[0];
    assert.equal(hello.type, fixtures.ExtensionHelloMessage.type);
    assert.deepEqual(Object.keys(hello).sort(), Object.keys(fixtures.ExtensionHelloMessage).sort());
    for (const name of ['protocolVersion', 'settingsSchemaVersion', 'supportedProtocolVersions', 'supportedSettingsSchemaVersions', 'capabilities']) {
      assert.deepEqual(hello[name], fixtures.ExtensionHelloMessage[name]);
    }
    assert.equal(hello.connectionIntent, 'user-requested');
    runtime.message(fixtures.AppHelloMessage);
    assert.equal(runtime.messages[1].type, fixtures.ExtensionControllerAcquireMessage.type);
    assert.equal(runtime.messages[1].protocolVersion, fixtures.ExtensionControllerAcquireMessage.protocolVersion);
    assert.deepEqual(Object.keys(runtime.messages[1]).sort(), Object.keys(fixtures.ExtensionControllerAcquireMessage).sort());
    runtime.message({...fixtures.AppControllerLeaseMessage, selectionStatus});
    assert.equal(runtime.messages[2].type, fixtures.ExtensionScreenshotChunkRequestMessage.type);
    assert.equal(runtime.messages[2].protocolVersion, fixtures.ExtensionScreenshotChunkRequestMessage.protocolVersion);
    assert.deepEqual(Object.keys(runtime.messages[2]).sort(), Object.keys(fixtures.ExtensionScreenshotChunkRequestMessage).sort());
    assert.equal(runtime.messages[2].controllerLeaseId, 'lease:stale-e2e');
    runtime.message({type: 'app.operation.failed', error: {code: 'stale-controller-lease'}});
    await Promise.resolve();
    assert.equal(runtime.reports.length, 1);
    assert.equal(runtime.reports[0].status, 'pass');
  }
  const rejected = start();
  rejected.message({type: 'app.operation.failed', error: {code: 'policy-denied'}});
  await Promise.resolve();
  assert.equal(rejected.reports[0].status, 'fail');
  const incompatible = start();
  incompatible.message({...fixtures.AppHelloMessage, protocolVersion: 1});
  await Promise.resolve();
  assert.equal(incompatible.reports[0].status, 'fail');
  assert.equal(incompatible.messages.length, 1);
}
run().then(() => process.stdout.write('Native browser probe contracts passed\n')).catch(error => { process.stderr.write(`${error.stack}\n`); process.exitCode = 1; });
