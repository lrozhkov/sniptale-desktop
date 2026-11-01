const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const {webcrypto} = require('node:crypto');
const source = fs.readFileSync(process.argv[2], 'utf8');
const extensionId = 'a'.repeat(32);
const clientId = 'b'.repeat(64);
async function check(reply, {action='connect', pin=null, extra={}}={}) {
  const nonces = new Set();
  let calls = 0;
  const result = await vm.runInNewContext(source, {
    __labAction: action, crypto: webcrypto, Blob, navigator: {userAgent:'Chrome/1.0'},
    setTimeout: callback => callback(),
    chrome: {storage:{local:{get: async key => ({[key]:pin})}}, runtime: {id:extensionId, sendMessage: async message => {
      const freshness = message.__sniptaleRuntimeFreshness;
      assert.ok(freshness && Number.isSafeInteger(freshness.issuedAtEpochMs));
      assert.ok(Math.abs(Date.now() - freshness.issuedAtEpochMs) < 5000);
      assert.match(freshness.nonce, /^[0-9a-f-]{36}$/);
      assert.ok(!nonces.has(freshness.nonce));
      nonces.add(freshness.nonce);
      assert.match(message.requestId, /^[0-9a-f-]{36}$/);
      return reply(message, ++calls);
    }}}, ...extra
  });
  return {result, calls};
}
(async () => {
  const reloadOrder = [];
  const reload = await check(() => { throw new Error('reload must not query or mutate native trust'); }, {
    action:'reload-runtime', extra:{chrome:{tabs:{create:async options => {
      assert.deepEqual(JSON.parse(JSON.stringify(options)), {url:'about:blank',active:false}); reloadOrder.push('keep-window');
    }},runtime:{reload:() => reloadOrder.push('reload')}}}
  });
  assert.equal(reload.result.passed,true);
  assert.equal(reload.calls,0);
  assert.deepEqual(reloadOrder,['keep-window','reload']);
  const lease = {protocolVersion:2, controllerLeaseId:'lease-1', status:'granted', selectionStatus:'selected',
    expiresAtEpochMs:Date.now()+60_000, controller:{extensionId,connectionId:'connection-1'}};
  const connected = {success:true, status:{appIdentity:{clientId,trustVersion:1}, connectionState:'connected', controllerLease:lease,
    settingsRevision:'desktop:1', effectiveSettings:{screenshots:{includeCursor:true}}, trayActions:null}};
  const valid = await check(() => connected);
  assert.equal(valid.result.passed, true);
  assert.equal(valid.result.selected, true);
  assert.equal(valid.calls, 2);
  const pending = await check((message, calls) => calls < 4 ? {...connected, status:{...connected.status, effectiveSettings:null}} : connected);
  assert.equal(pending.result.passed, true);
  assert.equal(pending.calls, 4);
  const mutation = await check(() => ({success:false}));
  assert.equal(mutation.result.category, 'runtime-mutation-rejected');
  assert.equal(mutation.calls, 1);
  const query = await check(message => message.type === 'NATIVE_APP_MUTATION' ? connected : {success:false});
  assert.equal(query.result.category, 'runtime-query-rejected');
  for (const patch of [{controllerLease:null}, {settingsRevision:null}, {effectiveSettings:null}, {appIdentity:null},
    {trayActions:{}}, {connectionState:'not-connected'}, {connectionState:'repair-required'}, {connectionState:'error'},
    {controllerLease:{...lease,expiresAtEpochMs:Date.now()-1}}, {controllerLease:{...lease,controller:{...lease.controller,extensionId:'z'.repeat(32)}}}]) {
    const denied = await check(() => ({...connected, status:{...connected.status, ...patch}}));
    assert.equal(denied.result.passed, false);
  }
  const observer = {...connected, status:{...connected.status, connectionState:'selection-required', settingsRevision:null, effectiveSettings:null,
    controllerLease:{...lease,status:'rejected',selectionStatus:'selection-required'}}};
  for (const [message, expected] of [['Invalid app.recording.stopped','app.recording.stopped'],
    ['Invalid app.settings.accepted','app.settings.accepted'], ['Invalid app.private.secret','unclassified'],
    ['https://private.invalid/token','unclassified']]) {
    const denied = await check(() => ({...connected,status:{...connected.status,connectionState:'error',error:{code:'malformed-message',message}}}));
    assert.equal(denied.result.rejectedMessage,expected);
    assert.ok(!JSON.stringify(denied.result).includes('private'));
  }
  const registered = await check(() => observer);
  assert.equal(registered.result.passed, true);
  assert.equal(registered.result.selected, false);
  const stillObserver = await check(() => observer, {action:'await-selected'});
  assert.equal(stillObserver.result.passed, false);
  const untrusted = {...observer,status:{...observer.status,connectionState:'trust-required'}};
  const trusted = await check((message, calls) => {
    if (message.operation === 'trust-client') { assert.equal(message.clientId, clientId); return {success:true}; }
    return calls < 4 ? untrusted : observer;
  });
  assert.equal(trusted.result.passed, true);
  assert.equal(trusted.calls, 4);
  const changedPin = await check(() => untrusted, {pin:{clientId:'c'.repeat(64)}});
  assert.equal(changedPin.result.category, 'unexpected-installation-identity');
  assert.equal(changedPin.calls, 2);

  const bytes = new Uint8Array([137,80,78,71,13,10,26,10,1,2,3]);
  const sha256 = Buffer.from(await webcrypto.subtle.digest('SHA-256', bytes)).toString('hex');
  const media = {id:'capture-1',tags:['native-app'],source:{kind:'stored-asset',assetId:'asset-1'}};
  const reference = {assetId:'asset-1',location:{kind:'opfs',objectKey:'objects/asset-1'},size:bytes.length,sha256,mimeType:'image/png'};
  const completion = {id:'capture-1',kind:'screenshot-complete',sha256,totalBytes:0,receivedBytes:0,chunkCount:0,metadata:{width:2,height:3}};
  async function mediaCheck(ref=reference, expectedBytes=bytes.length, fileBytes=bytes, action='verify-capture', completed=true, entry=media, backlog=false) {
    const rows = {native_transfer_sessions:completed?[completion]:[],media_library:[entry],asset_refs:[ref]};
    if (backlog) {
      rows.native_transfer_sessions.unshift({...completion,id:'older-cancelled',sha256:'0'.repeat(64)});
      rows.media_library.unshift({...media,id:'older-cancelled',kind:'screenshot',source:{kind:'screenshot'},blob:null,mimeType:'image/png',size:bytes.length});
    }
    const indexedDB = {databases:async()=>[{name:'sniptale-db'}],open:()=>{
      const request={};
      request.result={objectStoreNames:{contains:name=>name in rows},close:()=>{},transaction:name=>({objectStore:()=>({getAll:()=>{
        const read={result:rows[name]}; queueMicrotask(()=>read.onsuccess()); return read;
      }})})};
      queueMicrotask(()=>request.onsuccess()); return request;
    }};
    const objects={getFileHandle:async(name,options)=>{
      assert.equal(name,'asset-1'); assert.equal(options,undefined);
      return {getFile:async()=>new Blob([fileBytes])};
    }};
    const assets={getDirectoryHandle:async(name,options)=>{assert.equal(name,'objects'); assert.equal(options,undefined); return objects;}};
    const storage={getDirectory:async()=>({getDirectoryHandle:async(name,options)=>{
      assert.equal(name,'sniptale-assets'); assert.equal(options,undefined); return assets;
    }})};
    return check(()=>connected,{action,extra:{indexedDB, navigator:{storage},
      __labExpectedMedia:{sha256,totalBytes:expectedBytes,width:2,height:3},
      createImageBitmap:async()=>({width:2,height:3,close:()=>{}})}});
  }
  assert.equal((await mediaCheck()).result.passed,true);
  assert.equal((await mediaCheck(reference,bytes.length,bytes,'verify-not-delivered')).result.passed,false);
  assert.equal((await mediaCheck(reference,bytes.length,bytes,'verify-not-delivered',false)).result.passed,true);
  assert.equal((await check(()=>connected,{action:'verify-not-delivered'})).result.passed,false);
  assert.equal((await mediaCheck()).result.mimeType,'image/png');
  assert.equal((await mediaCheck({...reference,sha256:null})).result.passed,true);
  assert.equal((await mediaCheck({...reference,sha256:'0'.repeat(64)})).result.passed,false);
  assert.equal((await mediaCheck()).result.storageKind,'opfs');
  assert.equal((await mediaCheck(reference,bytes.length,bytes,'verify-capture',true,media,true)).result.passed,true);
  const inline = {...media,kind:'screenshot',source:{kind:'screenshot'},blob:new Blob([bytes],{type:'image/png'}),mimeType:'image/png',size:bytes.length,width:2,height:3};
  const inlineCheck = entry => mediaCheck(reference,bytes.length,bytes,'verify-capture',true,entry);
  assert.equal((await inlineCheck(inline)).result.passed,true);
  assert.equal((await inlineCheck(inline)).result.storageKind,'indexeddb-blob');
  for (const patch of [{blob:null},{blob:new Blob([new Uint8Array([137,80])])},{mimeType:'text/plain'},
    {size:bytes.length+1},{width:3},{height:4},{source:{kind:'unknown'}},
    {blob:new Blob([new Uint8Array([...bytes.slice(0,-1),99])])}]) {
    assert.equal((await inlineCheck({...inline,...patch})).result.passed,false);
  }
  assert.equal((await mediaCheck({...reference,mimeType:'text/plain'})).result.passed,false);
  assert.equal((await mediaCheck({...reference,location:{kind:'opfs',objectKey:'elsewhere'}})).result.passed,false);
  assert.equal((await mediaCheck(reference,bytes.length+1)).result.passed,false);
  assert.equal((await mediaCheck(reference,bytes.length,new Uint8Array([137,80]))).result.passed,false);
  assert.equal((await mediaCheck(reference,bytes.length,new Uint8Array([...bytes.slice(0,-1),99]))).result.passed,false);
  const videoBytes = new Uint8Array([0,0,0,16,102,116,121,112,1,2,3,4]);
  const videoHash = Buffer.from(await webcrypto.subtle.digest('SHA-256',videoBytes)).toString('hex');
  const rawSamples = [14745,18022,21298].map((x,index)=>({id:`native-cursor-${index}`,time:100*(index+1),x:Math.round(x/32767*1279),y:Math.round(20479/32767*799),visible:true}));
  const rawTelemetry = {viewport:{width:1280,height:800,scrollX:0,scrollY:0,devicePixelRatio:1},cursorTrack:{samples:rawSamples},
    actionEvents:rawSamples.map((sample,index)=>({id:`native-click-${index}`,kind:'CLICK',time:sample.time,duration:0,point:{x:sample.x,y:sample.y}})),signals:[]};
  const normalized = {recordingId:'native-video-1',viewport:rawTelemetry.viewport,cursorTrack:{samples:rawSamples.map(sample=>({...sample,time:sample.time/1000}))},
    actionEvents:rawTelemetry.actionEvents.map(event=>({...event,time:event.time/1000})),signals:[],provenance:{source:'native',normalizationVersion:1,timeUnit:'seconds',coordinateSpace:'desktop'}};
  async function videoCheck(patch={}) {
    const raw = structuredClone(rawTelemetry);
    const saved = structuredClone(normalized);
    const ref = {assetId:'video-asset',mimeType:'video/mp4',size:videoBytes.length,sha256:videoHash,location:{kind:'opfs',objectKey:'objects/video-asset'}};
    if (patch.change) patch.change({raw,saved,ref});
    const rows = {native_transfer_sessions:[{id:'native-video-1',kind:'recording-complete',sha256:videoHash,totalBytes:0,
      metadata:{width:1280,height:800,durationMs:1000,telemetry:raw}}],recordings:[{id:'native-video-1',assetId:'video-asset',mimeType:'video/mp4',size:videoBytes.length}],asset_refs:[ref],recording_telemetry:[saved]};
    const indexedDB={databases:async()=>[{name:'sniptale-db'}],open:()=>{
      const request={};request.result={objectStoreNames:{contains:name=>name in rows},close:()=>{},transaction:name=>({objectStore:()=>({getAll:()=>{
        const read={result:rows[name]};queueMicrotask(()=>read.onsuccess());return read;
      }})})};queueMicrotask(()=>request.onsuccess());return request;
    }};
    const objects={getFileHandle:async(name,options)=>{assert.equal(name,'video-asset');assert.equal(options,undefined);return {getFile:async()=>new Blob([patch.bytes??videoBytes])};}};
    const assets={getDirectoryHandle:async(name,options)=>{assert.equal(name,'objects');assert.equal(options,undefined);return objects;}};
    const storage={getDirectory:async()=>({getDirectoryHandle:async(name,options)=>{assert.equal(name,'sniptale-assets');assert.equal(options,undefined);return assets;}})};
    const settings={video:{telemetry:{collectCursor:!patch.optOut,collectClicks:true},advanced:{includeCursorInVideo:!!patch.embedded}}};
    const response={...connected,status:{...connected.status,effectiveSettings:settings}};
    const document={createElement:name=>{
      assert.equal(name,'video');
      const element={videoWidth:patch.width??1280,videoHeight:800,duration:1,removeAttribute:()=>{},load:()=>{}};
      Object.defineProperty(element,'src',{set:()=>{queueMicrotask(()=>patch.decodeError?element.onerror():element.onloadedmetadata());}});
      return element;
    }};
    return check(()=>response,{action:'verify-recording',extra:{indexedDB,navigator:{storage},document,
      URL:{createObjectURL:()=> 'blob:synthetic',revokeObjectURL:()=>{}},
      setTimeout:(callback,ms)=>{if(ms!==10000)callback();return 0;},
      __labExpectedMedia:{sha256:videoHash,totalBytes:videoBytes.length}}});
  }
  assert.equal((await videoCheck()).result.passed,true);
  assert.equal((await videoCheck({change:({ref})=>{ref.sha256=null;}})).result.passed,true);
  assert.equal((await videoCheck({change:({ref})=>{ref.sha256='0'.repeat(64);}})).result.passed,false);
  assert.equal((await videoCheck()).result.cursorSamples,3);
  assert.equal((await videoCheck()).result.normalization,'milliseconds-to-seconds-once');
  for (const patch of [{bytes:new Uint8Array([...videoBytes.slice(0,-1),99])},{width:1279},{decodeError:true},{optOut:true},{embedded:true},
    {change:({saved})=>{saved.cursorTrack.samples[1].time=200;}},
    {change:({saved})=>{saved.cursorTrack.samples[1].time=0.0002;}},
    {change:({raw,saved})=>{raw.cursorTrack.samples[1].x=1280;saved.cursorTrack.samples[1].x=1280;}},
    {change:({raw,saved})=>{raw.cursorTrack.samples.forEach(sample=>{sample.visible=false;});saved.cursorTrack.samples.forEach(sample=>{sample.visible=false;});}},
    {change:({saved})=>{saved.provenance.timeUnit='milliseconds';}},
    {change:({ref})=>{ref.mimeType='text/plain';}},
    {change:({ref})=>{ref.location.objectKey='elsewhere';}},
    {change:({saved})=>{saved.actionEvents[0].time=100;}}]) {
    assert.equal((await videoCheck(patch)).result.passed,false);
  }
  process.stdout.write('OK real extension freshness, explicit pin, observer/selected authority, immutable OPFS bytes, native hash and byte-count contracts\n');
})().catch(error => { process.stderr.write(error.stack + '\n'); process.exitCode = 1; });
