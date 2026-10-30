// A short-lived client attaches to the existing headful guest browser. It never launches a replacement browser.
const fs = require('node:fs');
const crypto = require('node:crypto');
const path = require('node:path');
const timer = setTimeout(() => fail('browser-scenario-deadline'), 75000);
function fail(category) { process.stderr.write(category + '\n', () => process.exit(1)); }
(async () => {
  const [port, request] = process.argv.slice(2);
  if (!['9223', '9224'].includes(port) || fs.statSync(request).size > 65536) throw Error('invalid-input');
  const input = JSON.parse(fs.readFileSync(request, 'utf8').replace(/^\uFEFF/, ''));
  if (typeof input.expression !== 'string' || Object.keys(input).length !== 1) throw Error('invalid-input');
  const root = 'C:/SniptaleLab/browser-tools';
  const files = JSON.parse(fs.readFileSync(root + '/runtime-integrity.json', 'utf8'));
  for (const [name, hash] of Object.entries(files)) {
    const file = path.resolve(root, name);
    if (!file.startsWith(path.resolve(root) + path.sep) || fs.lstatSync(file).isSymbolicLink() || crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex') !== hash) throw Error('tool-integrity');
  }
  const {chromium} = require(root + '/package');
  const browser = await chromium.connectOverCDP(`http://127.0.0.1:${port}`, {timeout:10000});
  const contexts = browser.contexts();
  if (contexts.length !== 1) throw Error('invalid-browser-context');
  const target = 'chrome-extension://elhndangpenapadjimfoodlaenpcndpo/apps/extension/src/settings/index.html';
  const pages = contexts[0].pages().filter(page => page.url() === target || page.url().startsWith(target + '?') || page.url().startsWith(target + '#'));
  if (!pages.length) throw Error('extension-page-missing');
  const value = await pages[pages.length - 1].evaluate(input.expression);
  if (value && typeof value === 'object') value.automation = {node:process.versions.node, playwrightCore:require(root + '/package/package.json').version};
  const result = JSON.stringify(value);
  if (!result || Buffer.byteLength(result) > 1048576) throw Error('invalid-result');
  clearTimeout(timer);
  // Exit disconnects this client without closing the persistent browser or its profile.
  process.stdout.write(result + '\n', () => process.exit(0));
})().catch(() => fail('browser-scenario-failed'));
