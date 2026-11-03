const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source = fs.readFileSync(process.argv[2], 'utf8');
const target = 'chrome-extension://elhndangpenapadjimfoodlaenpcndpo/apps/extension/src/settings/index.html';
async function run(patch = {}) {
  let output = '', error = '', code, connected = false, evaluated = false, closedDescriptors=0;
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
    isVisible:async()=>true,waitFor:async()=>{
      if(patch.editorNotReady||(selector==='archive'&&(patch.archiveMissing||(reloads&&patch.reopenedArchiveMissing))))throw Error('not-ready');
    },
    count:async()=>patch.archiveCount??1,first:()=>locator(selector),locator:()=>locator(selector),
    getByText:label=>{
      assert.match(selector,/history-cursor-row/);
      assert.ok(label.test(patch.archiveLanguage==='ru'?'История курсора в архиве':'Archived cursor history'));
      assert.equal(label.test('Cursor history unavailable'),false);
      return locator('archive');
    },
    click:async()=>{},boundingBox:async()=>({x:playheadX,y:20,width:8,height:8}),
    getAttribute:async name=>String(name==='aria-valuemax'?6:currentTime),
    press:async key=>{keys.push(key);if(!patch.seekStuck){currentTime=key==='Home'?0:6;playheadX=10+currentTime*20;}},
    evaluate:async()=>({width:patch.canvasWidth??640,height:400}),
    screenshot:async options=>screenshots.push(options.path),
  });
  const editor = {url:()=>((reloads&&patch.reopenedEditorUrl)||patch.editorUrl)??'chrome-extension://elhndangpenapadjimfoodlaenpcndpo/apps/extension/src/video-editor/index.html?id=video-1',
    bringToFront:async()=>{},waitForSelector:async()=>{if(patch.editorNotReady)throw Error('not-ready');},locator,
    getByRole:(role,options)=>({click:async()=>{assert.equal(role,'button');playing=options.name==='Play';}}),
    waitForTimeout:async()=>{if(playing&&!patch.playheadStuck){playheadX+=3;currentTime+=0.05;}},screenshot:async options=>screenshots.push(options.path),
    reload:async()=>{if(patch.reopenFailure)throw Error('reopen-failed');reloads++;currentTime=0;playheadX=10;},
  };
  const runtimeBytes='verified-runtime';
  const runtimeHash=require('node:crypto').createHash('sha256').update(runtimeBytes).digest('hex');
  const fileBytes=file=>Buffer.from(file.endsWith('runtime-integrity.json')
    ? (patch.integrityFile?JSON.stringify({'package/index.js':runtimeHash}):'{}')
    : file.endsWith('index.js')?(patch.corruptRuntime?'changed-runtime':runtimeBytes)
    : JSON.stringify(patch.input??{expression:'laboratory-expression'}));
  const context = {
    require: name => {
      if (name === 'node:fs') return {
        openSync:file => file,
        fstatSync:file => ({size:patch.size ?? fileBytes(file).length, isFile:()=>!patch.nonFile}),
        lstatSync:()=>({isSymbolicLink:()=>Boolean(patch.symbolicRuntime)}),
        readSync:(file,buffer,offset,length,position) => {
          const bytes=fileBytes(file);
          if(patch.truncatedFile) return 0;
          if(patch.growingFile && !file.endsWith('runtime-integrity.json')) return buffer.fill(65,offset,offset+length).length-offset;
          const count=Math.max(0,Math.min(length,bytes.length-position));
          bytes.copy(buffer,offset,position,position+count);
          return count;
        },
        closeSync:()=>{closedDescriptors++;},
      };
      if (name === 'C:/SniptaleLab/browser-tools/package') return {chromium:{connectOverCDP:async (endpoint, options) => {
        connected = true;
        assert.equal(endpoint, `http://127.0.0.1:${patch.port ?? '9223'}`);
        assert.equal(options.timeout, 10000);
        if (patch.connectFailure) throw Error('private connection details');
        return {contexts:() => patch.contexts ?? [{pages:() => patch.input?.scenario==='native-video-editor'?(patch.duplicateEditor?[page,editor,editor]:[page,editor]):[page]}], close:() => assert.fail('persistent browser must remain open')};
      }}};
      if (name.endsWith('/package/package.json')) return {version:'1.64.1'};
      if (name === 'node:crypto') return require('node:crypto');
      if (name === 'node:path') return require('node:path');
      if (name === './desktop-capture-playwright.cjs') return {
        desktopActions:require('../guest/windows/desktop-capture-playwright.cjs').desktopActions,
        captureScenario:async (_context, action) => {
          if (patch.desktopError) throw Error(patch.desktopError);
          return {passed:true, action};
        },
      };
      throw new Error('unexpected sandbox module');
    },
    Buffer, URL, setTimeout:() => 1, clearTimeout:() => {},
    process:{argv:['node','driver',patch.port ?? '9223','request.json'], versions:{node:'24.22.1'},
      stdout:{write:(text, done) => {output += text; done();}},
      stderr:{write:(text, done) => {error += text; done();}}, exit:value => {code = value;}},
  };
  await vm.runInNewContext(source, context);
  return {output, error, code, connected, evaluated, screenshots,keys,reloads,closedDescriptors};
}
(async () => {
  const good = await run();
  assert.equal(good.code, 0);
  assert.equal(JSON.parse(good.output).passed, true);
  const integrityGood=await run({integrityFile:true});
  assert.equal(integrityGood.code,0);
  for(const patch of [{integrityFile:true,corruptRuntime:true},{integrityFile:true,symbolicRuntime:true},{truncatedFile:true}]) {
    const rejected=await run(patch);
    assert.equal(rejected.code,1);
    assert.equal(rejected.connected,false);
    assert.ok(rejected.closedDescriptors>0);
  }
  const native = await run({input:{scenario:'native-video-editor'}});
  assert.equal(native.code,0);
  assert.equal(JSON.parse(native.output).playbackAdvanced,true);
  assert.equal(JSON.parse(native.output).seekStart,true);
  assert.equal(JSON.parse(native.output).seekEnd,true);
  assert.equal(JSON.parse(native.output).reopened,true);
  assert.equal(JSON.parse(native.output).archivedHistoryVisible,true);
  assert.equal(JSON.parse(native.output).reopenedHistoryVisible,true);
  assert.equal('historySegments' in JSON.parse(native.output),false);
  assert.equal((await run({input:{scenario:'native-video-editor'},archiveLanguage:'ru'})).code,0);
  assert.ok(native.keys.includes('Home')&&native.keys.includes('End'));
  assert.equal(native.reloads,1);
  assert.equal(native.screenshots.length,2);
  assert.ok(native.screenshots.every(name=>name.startsWith('screenshots/native-cursor-')&&name.endsWith('-chrome.png')));
  const editorBase='chrome-extension://elhndangpenapadjimfoodlaenpcndpo/apps/extension/src/video-editor/index.html';
  for(const query of ['?project=project-1&id=video-1','?id=video-1&project=project-1','?project=project-1&id=video%2D1']) {
    const canonical=await run({input:{scenario:'native-video-editor'},editorUrl:editorBase+query});
    assert.equal(canonical.code,0,'canonical project navigation must retain the verified recording binding');
    assert.equal(JSON.parse(canonical.output).playbackAdvanced,true);
    assert.equal(canonical.reloads,1);
  }
  const canonicalReopen=await run({input:{scenario:'native-video-editor'},reopenedEditorUrl:editorBase+'?project=project-1&id=video-1'});
  assert.equal(canonicalReopen.code,0);
  for (const action of require('../guest/windows/desktop-capture-playwright.cjs').desktopActions) {
    const desktop = await run({port:'9225', input:{scenario:'desktop-capture', action}});
    assert.equal(desktop.code, 0);
    assert.equal(JSON.parse(desktop.output).action, action);
    assert.equal((await run({port:'9223', input:{scenario:'desktop-capture', action}})).code, 1);
  }
  for (const message of ['desktop-settings-open-failed', 'desktop-settings-apply-failed', 'desktop-record-stop-failed', 'private host information']) {
    const failed = await run({port:'9225', input:{scenario:'desktop-capture', action:'settings-edit'}, desktopError:message});
    assert.equal(failed.code, 1);
    assert.equal(failed.error, message.startsWith('desktop-') ? `${message}\n` : 'browser-scenario-failed\n');
  }
  for (const patch of [{port:'1234'}, {port:'9225'}, {input:{scenario:'desktop-capture',action:'capture'}}, {port:'9225',input:{scenario:'desktop-capture',action:'unknown'}}, {port:'9225',input:{scenario:'desktop-capture',action:'capture',extra:true}}, {size:65537}, {nonFile:true}, {growingFile:true}, {connectFailure:true}, {contexts:[]},
    {contexts:[{},{}]}, {url:'https://untrusted.invalid/'}, {evaluateFailure:true}, {value:{data:'x'.repeat(1048576)}},
    {input:{scenario:'unknown'}},{input:{scenario:'native-video-editor',expression:'extra'}},
    ...[{recordingId:'../escape'},{editorUrl:'https://untrusted.invalid/'},{editorNotReady:true},{archiveMissing:true},{archiveCount:2},{playheadStuck:true},{canvasWidth:0},
      {seekStuck:true},{reopenFailure:true},{reopenedArchiveMissing:true},{duplicateEditor:true},
      ...['?project=project-1&id=other','?id=video-1&id=other','?id=video-1&project=one&project=two','?id=video-1&project=',
        '?id=video-1&review=other','?id=video-1&project='+('x'.repeat(257))].map(query=>({editorUrl:editorBase+query})),
      {editorUrl:editorBase.replace('elhndangpenapadjimfoodlaenpcndpo','aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa')+'?id=video-1'},
      {editorUrl:editorBase.replace('chrome-extension:','https:')+'?id=video-1'},
      {reopenedEditorUrl:editorBase+'?project=other&id=other'},
      {reopenedEditorUrl:'https://untrusted.invalid/?id=video-1'}].map(patch=>({...patch,input:{scenario:'native-video-editor'}}))]) {
    const failed = await run(patch);
    assert.equal(failed.code, 1);
    assert.equal(failed.output, '');
    assert.equal(failed.error, 'browser-scenario-failed\n');
  }
  process.stdout.write('OK Playwright attach, page selection, failure bounds, and persistent browser lifetime\n');
})().catch(error => {process.stderr.write(error.stack + '\n'); process.exitCode = 1;});
