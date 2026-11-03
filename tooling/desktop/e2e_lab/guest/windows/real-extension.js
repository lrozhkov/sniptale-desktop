// Runs in a real extension page through guest-local Playwright; all requests use the product runtime.
(() => async (verifyNativeHistory) => {
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
  const readKey = async (store,key) => {
    if (!(await indexedDB.databases()).some(db=>db.name==='sniptale-db')) return null;
    return new Promise((resolve,reject)=>{
      const request=indexedDB.open('sniptale-db');
      request.onerror=()=>reject(new Error('database-open-failed'));
      request.onsuccess=()=>{
        const db=request.result;
        if (!db.objectStoreNames.contains(store)) {db.close();resolve(null);return;}
        const read=db.transaction(store,'readonly').objectStore(store).get(key);
        read.onerror=()=>{db.close();reject(new Error('database-read-failed'));};
        read.onsuccess=()=>{db.close();resolve(read.result??null);};
      };
    });
  };
  const immutableHistories = async (session,entry,metadata,expected) => {
    const fail=category=>({passed:false,category});
    const intent=session.metadata.history;
    if (!intent || intent.phase==='pending' || !metadata) return {pending:true};
    if (intent.phase!=='complete' || intent.recordingId!==session.id || intent.recordingAssetId!==entry.assetId ||
        intent.videoSha256!==expected.sha256 || intent.width!==session.metadata.width || intent.height!==session.metadata.height) {
      return fail('native-history-parent-binding-invalid');
    }
    if (typeof verifyNativeHistory!=='function') return fail('native-history-verifier-missing');
    const width=session.metadata.width,height=session.metadata.height;
    const target=(x,y)=>[14745,18022,21298].findIndex(value=>
      Math.abs(x-value/32767*(width-1))<=4 && Math.abs(y-20479/32767*(height-1))<=4);
    let downs=0,ups=0,clicks=0,completions=0;
    const active=new Map(),proofs={};
    for (const kind of ['cursor','events']) {
      const cursor=kind==='cursor';
      const descriptor=metadata[cursor?'cursorTrajectory':'recordingEvents'];
      const snapshot=metadata[cursor?'cursorTrajectorySnapshot':'recordingEventsSnapshot'];
      const slot=intent[kind];
      const role=cursor?'cursor-trajectory':'recording-events';
      const id=descriptor?.[cursor?'trajectoryId':'eventStreamId'];
      if (!descriptor || !snapshot || !slot?.writer || slot.pull?.kind!=='published' ||
          typeof slot.writer.recordingId!=='string' || !slot.writer.recordingId || slot.writer.recordingId===session.id ||
          slot.writer[cursor?'trajectoryId':'eventStreamId']!==id ||
          typeof slot.writer.writerGeneration!=='string' || !slot.writer.writerGeneration ||
          snapshot.sourceRecordingId!==intent.recordingId ||
          (cursor && (snapshot.width!==width || snapshot.height!==height))) return fail('native-history-publication-missing');
      const fields=['format','formatVersion','algorithm','encoding','state','finalized','totalBlocks','totalRecords','totalBytes','streamSha256',cursor?'trajectoryId':'eventStreamId'];
      if (cursor) fields.push('totalSamples');
      if (!fields.every(key=>descriptor[key]===slot.descriptor?.[key])) return fail('native-history-native-summary-mismatch');
      const ref=snapshot.ref;
      if (!ref || typeof ref.assetId!=='string' || !ref.assetId || ref.assetId.length>256 || /[\\/]/.test(ref.assetId) || ['.','..'].includes(ref.assetId) ||
          ref.mimeType!=='application/octet-stream' || ref.location?.kind!=='opfs' || ref.location.objectKey!==`objects/${ref.assetId}` ||
          !Number.isSafeInteger(ref.size) || ref.size<=0 || ref.size>1048576 || ref.sha256!==descriptor.streamSha256 ||
          ref.size!==descriptor.totalBytes+4*descriptor.totalBlocks) return fail('native-history-snapshot-reference-invalid');
      const head=await readKey(cursor?'recording_cursor_trajectories':'recording_events',
        cursor?[session.id,id]:[session.id,id,'head',0]);
      if (!head || head.retired!==false || head.finalized!==true || head.snapshotAssetId!==ref.assetId ||
          head.stagingJournalId!==undefined || head.sourceRecordingId!==snapshot.sourceRecordingId ||
          head.recordingId!==session.id || head[cursor?'trajectoryId':'eventStreamId']!==id ||
          head.writerGeneration===slot.writer.writerGeneration || typeof head.writerGeneration!=='string' || !head.writerGeneration ||
          !['state','totalBlocks','totalRecords','totalBytes','streamSha256'].every(key=>head[key]===descriptor[key]) ||
          (cursor && head.totalSamples!==descriptor.totalSamples)) return fail('native-history-published-head-invalid');
      const owner=await readKey('asset_owners',['recording',session.id,role]);
      const registered=await readKey('asset_refs',ref.assetId);
      if (owner?.assetId!==ref.assetId || owner.ownerId!==session.id || owner.ownerKind!=='recording' || owner.role!==role ||
          !registered || !['assetId','createdAt','mimeType','sha256','size'].every(key=>registered[key]===ref[key]) ||
          registered.location?.kind!=='opfs' || registered.location.objectKey!==ref.location.objectKey) return fail('native-history-owner-binding-invalid');
      const objects=await(await(await navigator.storage.getDirectory()).getDirectoryHandle('sniptale-assets')).getDirectoryHandle('objects');
      const blob=await(await objects.getFileHandle(ref.assetId)).getFile();
      if (blob.size!==ref.size || blob.size>1048576) return fail('native-history-snapshot-file-size-invalid');
      const proof=await verifyNativeHistory({bytes:new Uint8Array(await blob.arrayBuffer()),descriptor,kind,sourceRecordingId:snapshot.sourceRecordingId,
        observe:record=>{
          if (cursor && record.kind==='segment' && (record.width!==width || record.height!==height)) return false;
          if (cursor && record.kind==='sample' && record.observed && record.visible) {
            const index=target(record.xQ8/256,record.yQ8/256);
            if (index>=0 && record.event==='down') downs|=1<<index;
            if (index>=0 && record.event==='up') ups|=1<<index;
          }
          if (!cursor && record.kind==='action' && record.action?.kind==='CLICK' && record.action.point) {
            const index=target(record.action.point.x*width,record.action.point.y*height);
            if (index>=0) {
              if (active.size>=32 || active.has(record.action.id)) return false;
              active.set(record.action.id,{index,down:record.action.data?.downTimeUs});
              clicks|=1<<index;
            }
          }
          if (!cursor && record.kind==='action-update' && active.has(record.actionId)) {
            const base=active.get(record.actionId),data=record.data;
            if (data?.completion!=='released' || !Number.isSafeInteger(base.down) || !Number.isSafeInteger(data.upTimeUs) ||
                data.upTimeUs<base.down || data.pressDurationUs!==data.upTimeUs-base.down || data.lastObservedTimeUs!==data.upTimeUs) return false;
            completions|=1<<base.index;active.delete(record.actionId);
          }
          return true;
        }});
      if (!proof.passed) return proof;
      proofs[kind]={blocks:proof.blocks,records:proof.records,bytes:proof.bytes,sha256:proof.streamSha256};
      if (cursor) proofs[kind].samples=proof.samples;
    }
    if (downs!==7 || ups!==7 || clicks!==7 || completions!==7 || active.size) return fail('native-history-physical-fixture-incomplete');
    return {passed:true,cursorSamples:proofs.cursor.samples,clickEvents:3,normalization:'microseconds-q8-verified',historyAttachments:proofs};
  };
  const action = globalThis.__labAction;
  // Parser messages are static literals. Never forward an arbitrary error message from the peer.
  const rejectedMessage = status => {
    const match = /^Invalid (app\.[a-z.]+)$/.exec(status?.error?.message ?? '');
    const known = ['app.hello', 'app.controller.lease', 'app.settings.accepted', 'app.pong',
      'app.screenshot.start', 'app.screenshot.chunk', 'app.screenshot.commit',
      'app.recording.started', 'app.recording.progress', 'app.recording.stopped', 'app.recording.chunk',
      'app.recording.attachment.manifest', 'app.recording.attachment.chunk', 'app.recording.attachment.available',
      'app.recording.events.manifest', 'app.recording.events.chunk', 'app.recording.events.available'];
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
  if (action === 'verify-recording' || action === 'verify-history-recording') {
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
        if (action==='verify-history-recording') {
          const metadata=await readKey('recording_telemetry',session.id);
          const histories=await immutableHistories(session,entry,metadata,expected);
          if (histories.pending) {await wait(500);continue;}
          if (!histories.passed) return histories;
          const legacyEvents=metadata.actionEvents??[],legacySamples=metadata.cursorTrack?.samples??[],legacySignals=metadata.signals??[];
          if (!Array.isArray(legacyEvents) || legacyEvents.length || !Array.isArray(legacySamples) || legacySamples.length ||
              !Array.isArray(legacySignals) || legacySignals.some(signal=>signal.kind!=='static-frame')) return {passed:false,category:'native-history-lifetime-arrays-present'};
          const response=await send({type:'NATIVE_APP_QUERY'});
          const settings=response.status?.effectiveSettings;
          if (!selected(response.status) || settings?.video?.telemetry?.collectCursor!==true || settings?.video?.telemetry?.collectClicks!==true ||
              settings?.video?.advanced?.includeCursorInVideo!==false) return {passed:false,category:'native-pointer-opt-in-settings-not-synchronized'};
          globalThis.__labVerifiedRecordingId=session.id;
          return {...histories,sha256:hash,bytes:bytes.length,width:session.metadata.width,height:session.metadata.height,durationMs:session.metadata.durationMs,
            mimeType:reference.mimeType,storageKind:'opfs',sourceCursorEmbedded:false};
        }
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
    return {passed:false,category:action==='verify-history-recording'?'native-history-publication-not-received':'native-video-not-received'};
  }
  if (action === 'open-recording-editor') {
    const id=globalThis.__labVerifiedRecordingId;
    if (typeof id!=='string'||!/^[A-Za-z0-9_-]{1,96}$/.test(id)) return {passed:false,category:'verified-native-recording-missing'};
    await chrome.tabs.create({url:`chrome-extension://${chrome.runtime.id}/apps/extension/src/video-editor/index.html?id=${encodeURIComponent(id)}`,active:true});
    return {passed:true};
  }
  return {passed:false, category:'unknown-laboratory-action'};
})()
