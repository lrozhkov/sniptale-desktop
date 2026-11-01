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
  // Parser messages are static literals. Never forward an arbitrary error message from the peer.
  const rejectedMessage = status => {
    const match = /^Invalid (app\.[a-z.]+)$/.exec(status?.error?.message ?? '');
    const known = ['app.hello', 'app.controller.lease', 'app.settings.accepted', 'app.pong',
      'app.screenshot.start', 'app.screenshot.chunk', 'app.screenshot.commit',
      'app.recording.started', 'app.recording.progress', 'app.recording.stopped', 'app.recording.chunk'];
    return match && known.includes(match[1]) ? match[1] : 'unclassified';
  };
  if (action === 'reload-runtime') {
    // Keep a browser window alive if reloading closes the extension page; retain the profile and pin.
    await chrome.tabs.create({url:'about:blank', active:false});
    setTimeout(() => chrome.runtime.reload(), 1000);
    return {passed:true};
  }
  const identityValid = status => /^[0-9a-f]{64}$/.test(status?.appIdentity?.clientId ?? '') && status.appIdentity.trustVersion === 1;
  const leaseValid = status => {
    const lease = status?.controllerLease;
    return identityValid(status) && lease?.protocolVersion === 2 && typeof lease.controllerLeaseId === 'string' && lease.controllerLeaseId.length > 0 &&
      lease.expiresAtEpochMs > Date.now() && lease.controller?.extensionId === chrome.runtime.id &&
      typeof lease.controller.connectionId === 'string' && lease.controller.connectionId.length > 0;
  };
  const selected = status => leaseValid(status) && status.connectionState === 'connected' &&
    status.controllerLease.status === 'granted' && status.controllerLease.selectionStatus === 'selected' &&
    typeof status.settingsRevision === 'string' && status.settingsRevision.length > 0 && status.effectiveSettings && status.trayActions === null;
  if (action === 'connect' || action === 'await-selected') {
    if (action === 'connect') {
      const mutation = await send({type: 'NATIVE_APP_MUTATION', operation: 'reconnect'});
      if (!mutation?.success) return {passed:false, category:'runtime-mutation-rejected'};
    }
    const pinKey = 'sniptale.nativeApp.trustedClient';
    const pin = (await chrome.storage.local.get(pinKey))[pinKey];
    let trustedId = pin?.clientId ?? null;
    let trustRequested = false;
    for (let i=0; i<120; i++) {
      const response = await send({type:'NATIVE_APP_QUERY'});
      if (!response?.success) return {passed:false, category:'runtime-query-rejected'};
      const status = response.status;
      if (trustedId && status?.appIdentity && status.appIdentity.clientId !== trustedId) return {passed:false, category:'unexpected-installation-identity'};
      if (status?.connectionState === 'trust-required' && !trustRequested) {
        if (!identityValid(status)) return {passed:false, category:'invalid-installation-identity'};
        // Explicit laboratory consent in the real Settings page; this is not an automatic product pin.
        trustedId = status.appIdentity.clientId;
        trustRequested = true;
        const consent = await send({type:'NATIVE_APP_MUTATION', operation:'trust-client', clientId:trustedId});
        if (!consent?.success) return {passed:false, category:'explicit-trust-rejected'};
        continue;
      }
      if (status && ['repair-required', 'error', 'incompatible-protocol', 'incompatible-settings', 'app-upgrade-required', 'extension-upgrade-required', 'policy-denied'].includes(status.connectionState)) {
        return {passed:false, connectionState:status.connectionState, category:status.error?.code ?? 'handshake-rejected', rejectedMessage:rejectedMessage(status)};
      }
      if (selected(status)) return {passed:true, selected:true, settingsApplied:true, connectionState:status.connectionState,
        selectionStatus:status.controllerLease.selectionStatus, browserVersion:navigator.userAgent.match(/(?:Edg|Chrome)\/[\d.]+/g)};
      if (action === 'connect' && leaseValid(status) && status.controllerLease.status !== 'granted' &&
          ['available', 'selection-required'].includes(status.controllerLease.selectionStatus) &&
          ['controlled-by-other-profile', 'selection-required'].includes(status.connectionState)) {
        return {passed:true, selected:false, settingsApplied:false, connectionState:status.connectionState, selectionStatus:status.controllerLease.selectionStatus};
      }
      await wait(500);
    }
    const response = await send({type:'NATIVE_APP_QUERY'});
    return {passed:false, connectionState:response.status?.connectionState, category:response.status?.error?.code ?? 'connection-timeout'};
  }
  if (action === 'before-capture') {
    const response = await send({type:'NATIVE_APP_QUERY'});
    if (!response.success || !selected(response.status)) return {passed:false, category:'selected-desktop-media-lease-unavailable'};
    globalThis.__labPriorTransfers = new Set((await readAll('native_transfer_sessions')).map(row => row.id));
    return {passed:true};
  }
  if (action === 'verify-not-delivered') {
    const expected = globalThis.__labExpectedMedia;
    if (!expected || !/^[0-9a-f]{64}$/.test(expected.sha256)) return {passed:false, category:'expected-native-media-missing'};
    const sessions = await readAll('native_transfer_sessions');
    const delivered = sessions.some(row => !globalThis.__labPriorTransfers?.has(row.id) && row.sha256 === expected.sha256 && (!expected.captureId || row.id === expected.captureId));
    return {passed:!delivered};
  }
  if (action === 'verify-capture') {
    const expected = globalThis.__labExpectedMedia;
    for (let i=0; i<80; i++) {
      const sessions = (await readAll('native_transfer_sessions')).filter(row => row.kind === 'screenshot-complete' && !globalThis.__labPriorTransfers?.has(row.id));
      const library = await readAll('media_library');
      for (const session of sessions) {
        if (session.sha256 !== expected?.sha256 || (expected?.captureId && session.id !== expected.captureId)) continue;
        const entry = library.find(row => row.id === session.id && row.tags?.includes('native-app'));
        if (!entry) continue;
        let reference;
        let blob;
        let storageKind;
        if (entry.source?.kind === 'screenshot' && entry.kind === 'screenshot') {
          if (!(entry.blob instanceof Blob) || entry.mimeType !== 'image/png' || !Number.isSafeInteger(entry.size) || entry.size <= 0 || entry.size > 64*1024*1024) {
            return {passed:false, category:'durable-image-blob-invalid'};
          }
          reference = {mimeType:entry.mimeType,size:entry.size,sha256:session.sha256};
          blob = entry.blob;
          storageKind = 'indexeddb-blob';
        } else if (entry.source?.kind === 'stored-asset') {
          reference = (await readAll('asset_refs')).find(row => row.assetId === entry.source.assetId);
          if (!reference || reference.mimeType !== 'image/png' || reference.location?.kind !== 'opfs' || typeof reference.assetId !== 'string' ||
            reference.assetId.length > 256 || /[\\/]/.test(reference.assetId) || ['.', '..'].includes(reference.assetId) ||
            reference.location.objectKey !== `objects/${reference.assetId}` || !Number.isSafeInteger(reference.size) || reference.size <= 0 || reference.size > 64*1024*1024) {
            return {passed:false, category:'immutable-asset-reference-invalid'};
          }
          const origin = await navigator.storage.getDirectory();
          const assets = await origin.getDirectoryHandle('sniptale-assets');
          const objects = await assets.getDirectoryHandle('objects');
          blob = await (await objects.getFileHandle(reference.assetId)).getFile();
          storageKind = 'opfs';
        } else {
          return {passed:false, category:'unsupported-durable-image-source'};
        }
        if (blob.size !== reference.size || blob.size > 64*1024*1024) return {passed:false, category:'immutable-asset-size-mismatch'};
        const bytes = new Uint8Array(await blob.arrayBuffer());
        const hash = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), b => b.toString(16).padStart(2,'0')).join('');
        const bitmap = await createImageBitmap(blob);
        const valid = expected && hash === expected.sha256 && bytes.length === expected.totalBytes &&
          hash === session.sha256 && (reference.sha256 === null || hash === reference.sha256) && bytes.length === reference.size &&
          bytes[0] === 137 && bytes[1] === 80 && bitmap.width === expected.width && bitmap.height === expected.height &&
          bitmap.width === session.metadata.width && bitmap.height === session.metadata.height &&
          (storageKind !== 'indexeddb-blob' || (bitmap.width === entry.width && bitmap.height === entry.height));
        const result = {passed:!!valid, sha256:hash, bytes:bytes.length, width:bitmap.width, height:bitmap.height, mimeType:reference.mimeType, storageKind};
        bitmap.close();
        return result;
      }
      await wait(500);
    }
    return {passed:false, category:'native-media-not-received'};
  }
  if (action === 'verify-recording') {
    const expected = globalThis.__labExpectedMedia;
    for (let i=0; i<80; i++) {
      const sessions = (await readAll('native_transfer_sessions')).filter(row => row.kind === 'recording-complete' && !globalThis.__labPriorTransfers?.has(row.id));
      const recordings = await readAll('recordings');
      const references = await readAll('asset_refs');
      for (const session of sessions) {
        if (session.sha256 !== expected?.sha256) continue;
        const entry = recordings.find(row => row.id === session.id);
        if (!entry) continue;
        const reference = references.find(row => row.assetId === entry.assetId);
        if (!reference || entry.mimeType !== 'video/mp4' || reference.mimeType !== 'video/mp4' || reference.location?.kind !== 'opfs' ||
            typeof reference.assetId !== 'string' || reference.assetId.length === 0 || reference.assetId.length > 256 || /[\\/]/.test(reference.assetId) || ['.','..'].includes(reference.assetId) ||
            reference.location.objectKey !== `objects/${reference.assetId}` || !Number.isSafeInteger(reference.size) || reference.size <= 0 || reference.size > 64*1024*1024) {
          return {passed:false,category:'immutable-native-video-reference-invalid'};
        }
        const objects = await (await (await navigator.storage.getDirectory()).getDirectoryHandle('sniptale-assets')).getDirectoryHandle('objects');
        const blob = await (await objects.getFileHandle(reference.assetId)).getFile();
        if (blob.size !== reference.size || blob.size !== entry.size || blob.size !== expected.totalBytes) return {passed:false,category:'native-video-size-mismatch'};
        const bytes = new Uint8Array(await blob.arrayBuffer());
        const hash = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)), b=>b.toString(16).padStart(2,'0')).join('');
        if (hash !== expected.sha256 || hash !== session.sha256) return {passed:false,category:'native-video-content-hash-mismatch'};
        if (reference.sha256 !== null && hash !== reference.sha256) return {passed:false,category:'native-video-reference-digest-mismatch'};
        if (String.fromCharCode(...bytes.slice(4,8)) !== 'ftyp') return {passed:false,category:'native-video-container-header-invalid'};
        const video = document.createElement('video');
        video.preload = 'metadata';
        const url = URL.createObjectURL(new Blob([bytes],{type:'video/mp4'}));
        try {
          const loaded = new Promise(resolve => {
            video.onloadedmetadata = ()=>resolve(true);
            video.onerror = ()=>resolve(false);
            setTimeout(()=>resolve(false),10000);
          });
          video.src = url;
          if (!(await loaded) || video.videoWidth !== session.metadata.width || video.videoHeight !== session.metadata.height ||
              !Number.isFinite(video.duration) || video.duration <= 0 || Math.abs(video.duration*1000-session.metadata.durationMs)>500) {
            return {passed:false,category:'native-video-decode-mismatch'};
          }
        } finally { video.removeAttribute('src'); video.load(); URL.revokeObjectURL(url); }
        const raw = session.metadata.telemetry;
        const telemetry = (await readAll('recording_telemetry')).find(row=>row.recordingId===session.id);
        const rawSamples = raw?.cursorTrack?.samples;
        const samples = telemetry?.cursorTrack?.samples;
        const clicks = raw?.actionEvents?.filter(event=>event.kind==='CLICK');
        if (!raw || !telemetry || !Array.isArray(rawSamples) || rawSamples.length<3 || rawSamples.length>4096 || !Array.isArray(samples) || samples.length!==rawSamples.length ||
            !Array.isArray(clicks) || clicks.length<3 || raw.viewport?.width!==session.metadata.width || raw.viewport?.height!==session.metadata.height ||
            raw.viewport?.scrollX!==0 || raw.viewport?.scrollY!==0 || raw.viewport?.devicePixelRatio!==1 ||
            !['width','height','scrollX','scrollY','devicePixelRatio'].every(key=>raw.viewport[key]===telemetry.viewport?.[key]) || telemetry.provenance?.source!=='native' ||
            telemetry.provenance?.normalizationVersion!==1 || telemetry.provenance?.timeUnit!=='seconds' || telemetry.provenance?.coordinateSpace!=='desktop') {
          return {passed:false,category:'native-pointer-telemetry-missing'};
        }
        const pointValid = point=>Number.isFinite(point.x)&&Number.isFinite(point.y)&&point.x>=0&&point.y>=0&&point.x<session.metadata.width&&point.y<session.metadata.height;
        if (!rawSamples.every((sample,index)=>Number.isSafeInteger(sample.time)&&sample.time>=0&&sample.time<=session.metadata.durationMs&&pointValid(sample)&&
          samples[index].id===sample.id&&samples[index].time===sample.time/1000&&samples[index].x===sample.x&&samples[index].y===sample.y&&samples[index].visible===sample.visible) ||
            telemetry.actionEvents.length!==raw.actionEvents.length || !raw.actionEvents.every((event,index)=>telemetry.actionEvents[index].time===event.time/1000&&telemetry.actionEvents[index].duration===event.duration/1000) ||
            telemetry.signals.length!==raw.signals.length || !raw.signals.every((signal,index)=>telemetry.signals[index].startTime===signal.startTime/1000&&telemetry.signals[index].endTime===signal.endTime/1000)) {
          return {passed:false,category:'native-pointer-time-or-coordinate-mismatch'};
        }
        for (const x of [14745,18022,21298]) {
          const close = point=>Math.abs(point.x-x/32767*(session.metadata.width-1))<=4&&Math.abs(point.y-20479/32767*(session.metadata.height-1))<=4;
          if (!rawSamples.some(sample=>sample.visible&&close(sample)) || !clicks.some(event=>event.point&&close(event.point))) {
            return {passed:false,category:'physical-pointer-fixture-not-captured'};
          }
        }
        const response = await send({type:'NATIVE_APP_QUERY'});
        const settings = response.status?.effectiveSettings;
        if (!selected(response.status)||settings?.video?.telemetry?.collectCursor!==true||settings?.video?.telemetry?.collectClicks!==true||settings?.video?.advanced?.includeCursorInVideo!==false) {
          return {passed:false,category:'native-pointer-opt-in-settings-not-synchronized'};
        }
        globalThis.__labVerifiedRecordingId=session.id;
        return {passed:true,sha256:hash,bytes:bytes.length,width:session.metadata.width,height:session.metadata.height,durationMs:session.metadata.durationMs,
          mimeType:reference.mimeType,storageKind:'opfs',cursorSamples:samples.length,clickEvents:clicks.length,normalization:'milliseconds-to-seconds-once',sourceCursorEmbedded:false};
      }
      await wait(500);
    }
    return {passed:false,category:'native-video-not-received'};
  }
  if (action === 'open-recording-editor') {
    const id=globalThis.__labVerifiedRecordingId;
    if (typeof id!=='string'||!/^[A-Za-z0-9_-]{1,96}$/.test(id)) return {passed:false,category:'verified-native-recording-missing'};
    await chrome.tabs.create({url:`chrome-extension://${chrome.runtime.id}/apps/extension/src/video-editor/index.html?id=${encodeURIComponent(id)}`,active:true});
    return {passed:true};
  }
  return {passed:false, category:'unknown-laboratory-action'};
})()
