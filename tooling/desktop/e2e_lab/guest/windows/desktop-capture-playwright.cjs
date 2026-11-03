// Exercise the packaged application through its real DOM and Tauri client.
const path = require('node:path');
const desktopActions = ['capture', 'reopen', 'window', 'settings-edit', 'settings-restore', 'record-start', 'record-stop', 'record-discard'];
function localDocument(url) {
  return ['http://tauri.localhost/', 'http://tauri.localhost/index.html', 'tauri://localhost/', 'tauri://localhost/index.html'].includes(url);
}
async function captureScenario(context, action, request) {
  if (!desktopActions.includes(action)) throw Error('invalid-desktop-action');
  const available = context.pages();
  const matching = available.filter(page => localDocument(page.url()));
  const pages = matching.length ? matching : available.filter(page => page.url() === 'about:blank');
  if (pages.length !== 1) throw Error('desktop-page-missing-or-ambiguous');
  const page = pages[0];
  await page.waitForURL(url => localDocument(url.href), {timeout:15000});
  page.setDefaultTimeout(15000);
  let stage = 'ready';
  try {
    await page.getByRole('toolbar', {name:'Capture toolbar', exact:true}).waitFor();
    if (action.startsWith('settings-')) {
      stage = 'settings-open';
      const opened = context.waitForEvent('page');
      await page.getByRole('button', {name:'Application settings', exact:true}).click();
      const settings = await opened;
      await settings.waitForURL(url => ['http://tauri.localhost/settings', 'tauri://localhost/settings'].includes(url.href));
      await settings.bringToFront();
      settings.setDefaultTimeout(15000);
      await settings.getByRole('button', {name:'Capture', exact:true}).click();
      const pointer = settings.getByRole('checkbox', {name:'Include pointer in screenshots', exact:true});
      const previous = await pointer.isChecked();
      stage = 'settings-apply';
      await pointer.setChecked(!previous);
      await settings.getByRole('button', {name:'Apply', exact:true}).click();
      await settings.getByText('Changes applied', {exact:true}).waitFor();
      await settings.screenshot({path:path.join(path.dirname(request), 'screenshots', `tauri-${action}.png`)});
      return {passed:true, previous, current:await pointer.isChecked()};
    }
    if (action.startsWith('record-')) {
      stage = action;
      if (action === 'record-start') {
        const switchMode = page.getByRole('button', {name:'Switch to video', exact:true});
        if (await switchMode.count()) await switchMode.click();
        const options = page.getByRole('button', {name:'Capture options', exact:true});
        await options.click();
        await page.getByRole('radio', {name:'Current screen', exact:true}).check();
        await page.getByRole('radio', {name:'Low', exact:true}).check();
        await options.click();
        await page.getByRole('button', {name:'Start recording', exact:true}).click();
        await page.getByRole('button', {name:'Pause', exact:true}).waitFor();
        await page.waitForFunction(() => {
          const value = globalThis.document.querySelector('.recording-clock')?.textContent.match(/(\d+):(\d+)/);
          return value && Number(value[1]) * 60 + Number(value[2]) >= 2;
        }, undefined, {timeout:15000});
      } else {
        // Reopening must project the same active Rust recording, rather than an idle toolbar.
        await page.getByRole('button', {name:'Pause', exact:true}).click();
        await page.getByRole('button', {name:'Resume', exact:true}).click();
        if (action === 'record-stop') {
          await page.getByRole('button', {name:'Stop', exact:true}).click();
          await page.getByText('Capture saved', {exact:true}).waitFor({timeout:45000});
        } else {
          await page.getByRole('button', {name:'Discard recording', exact:true}).click();
          const dialog = page.getByRole('dialog');
          await dialog.getByRole('button', {name:'Cancel', exact:true}).click();
          await page.getByRole('button', {name:'Pause', exact:true}).waitFor();
          await page.getByRole('button', {name:'Discard recording', exact:true}).click();
          await dialog.getByRole('button', {name:'Discard recording', exact:true}).click();
          await page.getByRole('button', {name:'Start recording', exact:true}).waitFor();
        }
      }
      await page.screenshot({path:path.join(path.dirname(request), 'screenshots', `tauri-${action}.png`)});
      await closePanel(page);
      return {passed:true, closed:true, recording:action === 'record-start', saved:action === 'record-stop', discarded:action === 'record-discard'};
    }
    const start = page.getByRole('button', {name:'Take screenshot', exact:true});
    if (action === 'capture' || action === 'window') {
      const options = page.getByRole('button', {name:'Capture options', exact:true});
      stage = 'options';
      await options.click();
      stage = 'source';
      await page.getByRole('radio', {name:action === 'window' ? 'Active window' : 'Current screen', exact:true}).check();
      await options.click();
      stage = 'capture';
      await start.click();
    }
    stage = 'saved';
    await page.getByText('Capture saved', {exact:true}).waitFor({timeout:25000});
    if (!(await start.isEnabled())) throw Error('desktop-command-remained-pending');
    await page.screenshot({path:path.join(path.dirname(request), 'screenshots', `tauri-${action}.png`)});
    stage = 'close';
    await closePanel(page);
    return {passed:true, saved:true, closed:true, restored:action === 'reopen'};
  } catch {
    if (!page.isClosed()) {
      try { await page.screenshot({path:path.join(path.dirname(request), 'screenshots', 'tauri-failure.png')}); }
      catch { /* Keep the original failed stage when the renderer cannot provide a screenshot. */ }
    }
    throw Error(`desktop-${stage}-failed`);
  }
}
async function closePanel(page) {
  await Promise.all([
    page.waitForEvent('close'),
    page.getByRole('button', {name:'Hide capture toolbar', exact:true}).click({noWaitAfter:true})
      .catch(error => { if (!page.isClosed()) throw error; }),
  ]);
}
module.exports = {captureScenario, localDocument, desktopActions};
