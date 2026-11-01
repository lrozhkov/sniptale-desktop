// A short-lived client attaches to the existing headful guest browser. It never launches a replacement browser.
const fs = require('node:fs');
const nodeCrypto = require('node:crypto');
const path = require('node:path');
const timer = setTimeout(() => fail('browser-scenario-deadline'), 75000);
function fail(category) { process.stderr.write(category + '\n', () => process.exit(1)); }
async function inspectNativeEditor(context, settingsPage, port, request) {
  const id = await settingsPage.evaluate(() => globalThis.__labVerifiedRecordingId);
  if (typeof id !== 'string' || !/^[A-Za-z0-9_-]{1,96}$/.test(id)) throw Error('verified-recording-missing');
  const target = `chrome-extension://elhndangpenapadjimfoodlaenpcndpo/apps/extension/src/video-editor/index.html?id=${encodeURIComponent(id)}`;
  let pages = [];
  for (let attempt=0;attempt<40;attempt++) {
    pages = context.pages().filter(page=>page.url()===target);
    if (pages.length) break;
    await settingsPage.waitForTimeout(250);
  }
  if (pages.length!==1) throw Error('native-editor-page-missing-or-ambiguous');
  const editor = pages[0];
  await editor.bringToFront();
  await editor.waitForSelector('[data-ui="video-editor.timeline.surface"]',{timeout:20000});
  const row = editor.locator('[data-ui="video-editor.timeline.history-cursor-row"]');
  if (!(await row.isVisible())) {
    await editor.locator('[data-ui="video-editor.timeline.toolbar.telemetry-lane"]').click();
  }
  const segments = editor.locator('[data-ui="video-editor.timeline.history-cursor-row"] [data-timeline-effect-segment]');
  await segments.first().waitFor({state:'visible',timeout:15000});
  const count = await segments.count();
  if (count<1||count>4096) throw Error('native-editor-cursor-segments-missing');
  const canvas = editor.locator('[data-ui="video.preview.stage.root"] canvas[data-preview-stage-canvas]');
  await canvas.waitFor({state:'visible',timeout:15000});
  const playhead = editor.locator('[data-ui="video-editor.timeline.playhead-handle"]');
  const before = await playhead.boundingBox();
  if (!before) throw Error('native-editor-playhead-missing');
  await editor.getByRole('button',{name:'Play',exact:true}).click();
  let moved = false;
  for(let attempt=0;attempt<60;attempt++) {
    await editor.waitForTimeout(250);
    const after = await playhead.boundingBox();
    if(after&&after.x>before.x+1) {moved=true;break;}
  }
  if(!moved) throw Error('native-editor-playback-did-not-advance');
  await editor.waitForTimeout(1500);
  await editor.getByRole('button',{name:'Pause',exact:true}).click();
  await editor.waitForTimeout(300);
  const size = await canvas.evaluate(element=>({width:element.width,height:element.height}));
  if(!Number.isInteger(size.width)||!Number.isInteger(size.height)||size.width<=0||size.height<=0) throw Error('native-editor-preview-canvas-empty');
  const browser = port==='9223'?'chrome':'edge';
  const directory = path.join(path.dirname(request),'screenshots');
  await canvas.screenshot({path:path.join(directory,`native-cursor-preview-${browser}.png`)});
  await editor.screenshot({path:path.join(directory,`native-cursor-editor-${browser}.png`)});
  return {passed:true,historySegments:count,canvasWidth:size.width,canvasHeight:size.height,playbackAdvanced:true};
}
(async () => {
  const [port, request] = process.argv.slice(2);
  if (!['9223', '9224'].includes(port) || fs.statSync(request).size > 65536) throw Error('invalid-input');
  const input = JSON.parse(fs.readFileSync(request, 'utf8').replace(/^\uFEFF/, ''));
  const editorScenario = input.scenario === 'native-video-editor';
  if (Object.keys(input).length !== 1 || (!editorScenario && typeof input.expression !== 'string')) throw Error('invalid-input');
  const root = 'C:/SniptaleLab/browser-tools';
  const files = JSON.parse(fs.readFileSync(root + '/runtime-integrity.json', 'utf8'));
  for (const [name, hash] of Object.entries(files)) {
    const file = path.resolve(root, name);
    if (!file.startsWith(path.resolve(root) + path.sep) || fs.lstatSync(file).isSymbolicLink() || nodeCrypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex') !== hash) throw Error('tool-integrity');
  }
  const {chromium} = require(root + '/package');
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, {timeout:10000});
  const contexts = browser.contexts();
  if (contexts.length !== 1) throw Error('invalid-browser-context');
  const target = 'chrome-extension://elhndangpenapadjimfoodlaenpcndpo/apps/extension/src/settings/index.html';
  const pages = contexts[0].pages().filter(page => page.url() === target || page.url().startsWith(target + '?') || page.url().startsWith(target + '#'));
  if (!pages.length) throw Error('extension-page-missing');
  const value = editorScenario ? await inspectNativeEditor(contexts[0],pages[pages.length-1],port,request) : await pages[pages.length - 1].evaluate(input.expression);
  if (value && typeof value === 'object') value.automation = {node:process.versions.node, playwrightCore:require(root + '/package/package.json').version};
  const result = JSON.stringify(value);
  if (!result || Buffer.byteLength(result) > 1048576) throw Error('invalid-result');
  clearTimeout(timer);
  // Exit disconnects this client without closing the persistent browser or its profile.
  process.stdout.write(result + '\n', () => process.exit(0));
})().catch(() => fail('browser-scenario-failed'));
