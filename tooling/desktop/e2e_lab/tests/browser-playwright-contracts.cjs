const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(process.argv[2], 'utf8');
const target = 'chrome-extension://elhndangpenapadjimfoodlaenpcndpo/apps/extension/src/settings/index.html';
async function run(patch = {}) {
  let output = '', error = '', code, connected = false, evaluated = false;
  const page = {url:() => patch.url ?? target, evaluate:async expression => {
    evaluated = true;
    assert.equal(expression, 'laboratory-expression');
    if (patch.evaluateFailure) throw Error('private failure must not be logged');
    return patch.value ?? {passed:true};
  }};
  const context = {
    require: name => {
      if (name === 'node:fs') return {
        statSync:() => ({size:patch.size ?? 100}),
        readFileSync:path => path.endsWith('runtime-integrity.json') ? '{}' : '{"expression":"laboratory-expression"}',
      };
      if (name === 'C:/SniptaleLab/browser-tools/package') return {chromium:{connectOverCDP:async (endpoint, options) => {
        connected = true;
        assert.equal(endpoint, 'http://127.0.0.1:9223');
        assert.equal(options.timeout, 10000);
        if (patch.connectFailure) throw Error('private connection details');
        return {contexts:() => patch.contexts ?? [{pages:() => [page]}], close:() => assert.fail('persistent browser must remain open')};
      }}};
      if (name.endsWith('/package/package.json')) return {version:'1.64.1'};
      if (name === 'node:crypto') return require('node:crypto');
      if (name === 'node:path') return require('node:path');
      throw new Error('unexpected sandbox module');
    },
    Buffer, setTimeout:() => 1, clearTimeout:() => {},
    process:{argv:['node','driver',patch.port ?? '9223','request.json'], versions:{node:'24.22.1'},
      stdout:{write:(text, done) => {output += text; done();}},
      stderr:{write:(text, done) => {error += text; done();}}, exit:value => {code = value;}},
  };
  await vm.runInNewContext(source, context);
  return {output, error, code, connected, evaluated};
}
(async () => {
  const good = await run();
  assert.equal(good.code, 0);
  assert.equal(JSON.parse(good.output).passed, true);
  for (const patch of [{port:'1234'}, {size:65537}, {connectFailure:true}, {contexts:[]},
    {contexts:[{},{}]}, {url:'https://untrusted.invalid/'}, {evaluateFailure:true}, {value:{data:'x'.repeat(1048576)}}]) {
    const failed = await run(patch);
    assert.equal(failed.code, 1);
    assert.equal(failed.output, '');
    assert.equal(failed.error, 'browser-scenario-failed\n');
  }
  process.stdout.write('OK Playwright attach, page selection, failure bounds, and persistent browser lifetime\n');
})().catch(error => {process.stderr.write(error.stack + '\n'); process.exitCode = 1;});
