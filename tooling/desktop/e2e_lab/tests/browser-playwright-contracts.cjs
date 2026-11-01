const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(process.argv[2], 'utf8');
const target = 'chrome-extension://elhndangpenapadjimfoodlaenpcndpo/apps/extension/src/settings/index.html';
async function run(patch = {}) {
  let output = '', error = '', code, connected = false, evaluated = false;
  let playheadX=10, playing=false,currentTime=0,reloads=0;
  const keys=[];
  const screenshots=[];
  const page = {url:() => patch.url ?? target, evaluate:async expression => {
    evaluated = true;
    if (typeof expression==='function') return patch.recordingId ?? 'video-1';
    assert.equal(expression, 'laboratory-expression');
    if (patch.evaluateFailure) throw Error('private failure must not be logged');
    return patch.value ?? {passed:true};
  },waitForTimeout:async()=>{}};
  const locator = selector => ({
    isVisible:async()=>true,waitFor:async()=>{if(patch.editorNotReady)throw Error('not-ready');},
    count:async()=>reloads&&patch.reopenedSegmentsMissing?0:patch.segments??1,first:()=>locator(selector),locator:()=>locator(selector),
    click:async()=>{},boundingBox:async()=>({x:playheadX,y:20,width:8,height:8}),
    getAttribute:async name=>String(name==='aria-valuemax'?6:currentTime),
    press:async key=>{keys.push(key);if(!patch.seekStuck){currentTime=key==='Home'?0:6;playheadX=10+currentTime*20;}},
    evaluate:async()=>({width:patch.canvasWidth??640,height:400}),
    screenshot:async options=>screenshots.push(options.path),
  });
  const editor = {url:()=>patch.editorUrl??'chrome-extension://elhndangpenapadjimfoodlaenpcndpo/apps/extension/src/video-editor/index.html?id=video-1',
    bringToFront:async()=>{},waitForSelector:async()=>{if(patch.editorNotReady)throw Error('not-ready');},locator,
    getByRole:(role,options)=>({click:async()=>{assert.equal(role,'button');playing=options.name==='Play';}}),
    waitForTimeout:async()=>{if(playing&&!patch.playheadStuck){playheadX+=3;currentTime+=0.05;}},screenshot:async options=>screenshots.push(options.path),
    reload:async()=>{if(patch.reopenFailure)throw Error('reopen-failed');reloads++;currentTime=0;playheadX=10;},
  };
  const context = {
    require: name => {
      if (name === 'node:fs') return {
        statSync:() => ({size:patch.size ?? 100}),
        readFileSync:path => path.endsWith('runtime-integrity.json') ? '{}' : JSON.stringify(patch.input??{expression:'laboratory-expression'}),
      };
      if (name === 'C:/SniptaleLab/browser-tools/package') return {chromium:{connectOverCDP:async (endpoint, options) => {
        connected = true;
        assert.equal(endpoint, 'http://127.0.0.1:9223');
        assert.equal(options.timeout, 10000);
        if (patch.connectFailure) throw Error('private connection details');
        return {contexts:() => patch.contexts ?? [{pages:() => patch.input?.scenario==='native-video-editor'?[page,editor]:[page]}], close:() => assert.fail('persistent browser must remain open')};
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
  return {output, error, code, connected, evaluated, screenshots,keys,reloads};
}
(async () => {
  const good = await run();
  assert.equal(good.code, 0);
  assert.equal(JSON.parse(good.output).passed, true);
  const native = await run({input:{scenario:'native-video-editor'}});
  assert.equal(native.code,0);
  assert.equal(JSON.parse(native.output).playbackAdvanced,true);
  assert.equal(JSON.parse(native.output).seekStart,true);
  assert.equal(JSON.parse(native.output).seekEnd,true);
  assert.equal(JSON.parse(native.output).reopened,true);
  assert.ok(native.keys.includes('Home')&&native.keys.includes('End'));
  assert.equal(native.reloads,1);
  assert.equal(native.screenshots.length,2);
  assert.ok(native.screenshots.every(name=>name.startsWith('screenshots/native-cursor-')&&name.endsWith('-chrome.png')));
  for (const patch of [{port:'1234'}, {size:65537}, {connectFailure:true}, {contexts:[]},
    {contexts:[{},{}]}, {url:'https://untrusted.invalid/'}, {evaluateFailure:true}, {value:{data:'x'.repeat(1048576)}},
    {input:{scenario:'unknown'}},{input:{scenario:'native-video-editor',expression:'extra'}},
    ...[{recordingId:'../escape'},{editorUrl:'https://untrusted.invalid/'},{editorNotReady:true},{segments:0},{playheadStuck:true},{canvasWidth:0},
      {seekStuck:true},{reopenFailure:true},{reopenedSegmentsMissing:true}].map(patch=>({...patch,input:{scenario:'native-video-editor'}}))]) {
    const failed = await run(patch);
    assert.equal(failed.code, 1);
    assert.equal(failed.output, '');
    assert.equal(failed.error, 'browser-scenario-failed\n');
  }
  process.stdout.write('OK Playwright attach, page selection, failure bounds, and persistent browser lifetime\n');
})().catch(error => {process.stderr.write(error.stack + '\n'); process.exitCode = 1;});
