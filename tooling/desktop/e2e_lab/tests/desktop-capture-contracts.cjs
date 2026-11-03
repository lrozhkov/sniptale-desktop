const assert = require('node:assert/strict');
const {captureScenario, localDocument} = require('../guest/windows/desktop-capture-playwright.cjs');
function fixture(patch = {}) {
  const actions = [];
  let closed = false, close;
  const page = {
    url:() => patch.initialUrl ?? patch.url ?? 'http://tauri.localhost/',
    waitForURL:async predicate => { if (!predicate(new URL(patch.url ?? 'http://tauri.localhost/'))) throw Error('foreign target'); },
    setDefaultTimeout:() => {}, isClosed:() => closed,
    screenshot:async value => actions.push(['screenshot', value.path]),
    waitForEvent:name => { assert.equal(name, 'close'); return new Promise(resolve => {close = resolve;}); },
    getByRole:(role, options) => ({
      waitFor:async() => actions.push(['ready', role]),
      check:async() => { if (patch.sourceFailure) throw Error('private backend information'); actions.push(['check', options.name]); },
      isEnabled:async() => !patch.pending,
      click:async() => {
        actions.push(['click', options.name]);
        if (options.name === 'Hide capture toolbar') {
          if (patch.closeFailure) throw Error('private close failure');
          closed = true; close();
          if (patch.closeDuringClick) throw Error('target closed');
        }
      },
    }),
    getByText:name => ({waitFor:async() => { if (patch.noSave) throw Error('not saved'); actions.push(['status', name]); }}),
  };
  return {actions, context:{pages:() => patch.pages ?? [page]}};
}
(async () => {
  for (const value of ['http://tauri.localhost/', 'tauri://localhost/index.html']) assert.ok(localDocument(value));
  for (const value of ['https://untrusted.invalid/', 'http://tauri.localhost/sandbox', 'http://tauri.localhost/?preview=1']) assert.equal(localDocument(value), false);
  for (const action of ['capture', 'reopen', 'window']) {
    const run = fixture({closeDuringClick:true});
    const result = await captureScenario(run.context, action, 'out/request.json');
    assert.equal(result.closed, true); assert.equal(result.restored, action === 'reopen');
    assert.equal(run.actions.filter(item => item[0] === 'click' && item[1] === 'Take screenshot').length, action === 'reopen' ? 0 : 1);
    assert.deepEqual(run.actions.filter(item => item[0] === 'check').map(item => item[1]), action === 'reopen' ? [] : [action === 'window' ? 'Active window' : 'Current screen']);
    assert.equal(run.actions.at(-1)[1], 'Hide capture toolbar');
  }
  for (const patch of [{sourceFailure:true}, {pending:true}, {noSave:true}, {pages:[]}, {pages:[{url:() => 'http://tauri.localhost/'},{url:() => 'tauri://localhost/'}]}, {url:'https://untrusted.invalid/'}]) {
    const run = fixture(patch);
    await assert.rejects(captureScenario(run.context, 'capture', 'out/request.json'));
    assert.equal(run.actions.some(item => item[1] === 'Hide capture toolbar'), false, 'Failure must retain the panel for evidence');
  }
  const closeFailed = fixture({closeFailure:true});
  await assert.rejects(captureScenario(closeFailed.context, 'capture', 'out/request.json'), {message:'desktop-close-failed'});
  assert.equal(closeFailed.context.pages()[0].isClosed(), false);
  const shared = fixture();
  const withSettings = {pages:() => [{url:() => 'http://tauri.localhost/settings'}, ...shared.context.pages()]};
  assert.equal((await captureScenario(withSettings, 'capture', 'out/request.json')).saved, true);
  assert.equal((await captureScenario(fixture({initialUrl:'about:blank'}).context, 'capture', 'out/request.json')).saved, true);
  await assert.rejects(captureScenario(fixture({initialUrl:'about:blank', url:'https://untrusted.invalid/'}).context, 'capture', 'out/request.json'));
  await assert.rejects(captureScenario({pages:() => [{url:() => 'http://tauri.localhost/settings'}]}, 'capture', 'out/request.json'), {message:'desktop-page-missing-or-ambiguous'});
  const failed = fixture({sourceFailure:true});
  await assert.rejects(captureScenario(failed.context, 'capture', 'out/request.json'), {message:'desktop-source-failed'});
  await assert.rejects(captureScenario(fixture().context, 'unknown', 'out/request.json'));
  process.stdout.write('OK real desktop target, one command, saved receipt, close/reopen, and fail-closed driver states\n');
})().catch(error => {process.stderr.write(error.stack + '\n'); process.exitCode = 1;});
