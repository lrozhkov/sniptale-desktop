// Runs in a real extension page through guest-local Playwright; all requests use the product runtime.
// eslint-disable-next-line @typescript-eslint/no-floating-promises -- Playwright and the VM contract harness await this expression and propagate rejection.
(async () => {
  const send = message => chrome.runtime.sendMessage({...message, requestId: crypto.randomUUID(),
    __sniptaleRuntimeFreshness: {issuedAtEpochMs: Date.now(), nonce: crypto.randomUUID()}});
  const wait = ms => new Promise(resolve => { setTimeout(resolve, ms); });
  const readAll = async store => {
    if (!(await indexedDB.databases()).some(db => db.name === 'sniptale-db')) return [];
    return new Promise((resolve, reject) => {
    const request = indexedDB.open('sniptale-db');
    request.onerror = () => reject(new Error('database-open-failed'));
    request.onsuccess = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(store)) {db.close(); resolve([]); return;}
      const tx = db.transaction(store, 'readonly');
      const read = tx.objectStore(store).getAll();
      read.onerror = () => {db.close(); reject(new Error('database-read-failed'));};
      read.onsuccess = () => {db.close(); resolve(read.result);};
    };
    });
  };
  const action = globalThis.__labAction;
  if (action === 'connect') {
    const mutation = await send({type: 'NATIVE_APP_MUTATION', operation: 'reconnect'});
    if (!mutation?.success) return {passed:false, category:'runtime-mutation-rejected'};
    for (let i=0; i<80; i++) {
      const response = await send({type:'NATIVE_APP_QUERY'});
      if (!response?.success) return {passed:false, category:'runtime-query-rejected'};
      const status = response.status;
      if (status && ['repair-required', 'error', 'incompatible-protocol', 'incompatible-settings', 'app-upgrade-required', 'extension-upgrade-required', 'policy-denied', 'controlled-by-other-profile'].includes(status.connectionState)) {
        return {passed:false, connectionState:status.connectionState, category:status.error?.code ?? 'handshake-rejected'};
      }
      if (response.success && status?.connectionState === 'connected' && status.controllerLease && status.settingsRevision && status.effectiveSettings) {
        return {passed:true, connectionState:status.connectionState, settingsApplied:true,
          browserVersion:navigator.userAgent.match(/(?:Edg|Chrome)\/[\d.]+/g)};
      }
      await wait(500);
    }
    const response = await send({type:'NATIVE_APP_QUERY'});
    return {passed:false, connectionState:response.status?.connectionState, category:response.status?.error?.code ?? 'connection-timeout'};
  }
  if (action === 'before-capture') {
    const response = await send({type:'NATIVE_APP_QUERY'});
    const screen = response.status?.trayActions?.actions.find(action => action.id === 'capture-screenshot-screen');
    if (!response.success || response.status?.connectionState !== 'connected' || !response.status?.effectiveSettings || screen?.enabled !== true) {
      return {passed:false, category:'native-screen-action-unavailable'};
    }
    globalThis.__labPriorTransfers = new Set((await readAll('native_transfer_sessions')).map(row => row.id));
    return {passed:true};
  }
  if (action === 'verify-capture') {
    for (let i=0; i<80; i++) {
      const sessions = (await readAll('native_transfer_sessions')).filter(row => row.kind === 'screenshot-complete' && !globalThis.__labPriorTransfers?.has(row.id));
      const library = await readAll('media_library');
      for (const session of sessions) {
        const entry = library.find(row => row.id === session.id && row.tags?.includes('native-app'));
        if (!entry || !(entry.blob instanceof Blob)) continue;
        const bytes = new Uint8Array(await entry.blob.arrayBuffer());
        const hash = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), b => b.toString(16).padStart(2,'0')).join('');
        const bitmap = await createImageBitmap(entry.blob);
        // Completed product sessions deliberately clear chunk byte counts; the retained native
        // digest and capture geometry remain the independent integrity expectations.
        const valid = hash === session.sha256 && bytes.length > 0 && bytes[0] === 137 && bytes[1] === 80 && bitmap.width === session.metadata.width && bitmap.height === session.metadata.height;
        const result = {passed:valid, sha256:hash, bytes:bytes.length, width:bitmap.width, height:bitmap.height, mimeType:entry.blob.type};
        bitmap.close();
        return result;
      }
      await wait(500);
    }
    return {passed:false, category:'native-media-not-received'};
  }
  return {passed:false, category:'unknown-laboratory-action'};
})()
