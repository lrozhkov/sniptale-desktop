const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const vm = require('node:vm');
const {createHash,webcrypto} = require('node:crypto');
const verify = vm.runInNewContext(fs.readFileSync(process.argv[2],'utf8'), {
  Uint8Array,DataView,TextDecoder,crypto:webcrypto,
});
const fixtures = path.resolve(__dirname,'../../../../contracts/native/fixtures');
const read = name => JSON.parse(fs.readFileSync(path.join(fixtures,name),'utf8'));
const digest = bytes => createHash('sha256').update(bytes).digest('hex');
function framed(text) {
  const body = Buffer.from(text,'utf8');
  const header = Buffer.alloc(4);
  header.writeUInt32LE(body.length);
  return new Uint8Array(Buffer.concat([header,body]));
}
function request(kind,blocks,descriptor={}) {
  const bytes = new Uint8Array(Buffer.concat(blocks.map(block=>framed(block.utf8))));
  const first = blocks[0].value;
  const cursor = kind==='cursor';
  return {kind,bytes,sourceRecordingId:first.recordingId,descriptor:{
    format:cursor?'cursor-trajectory-v1':'recording-events-v1',formatVersion:1,
    algorithm:cursor?'sed-window-v1':'lossless-v1',encoding:cursor?'json-ascii-int-v1':'json-utf8-v1',
    ...(cursor?{trajectoryId:first.trajectoryId,totalSamples:blocks.flatMap(block=>block.value.records).filter(record=>record.kind==='sample').length}:{eventStreamId:first.eventStreamId}),
    state:'complete',finalized:true,totalBlocks:blocks.length,
    totalRecords:blocks.reduce((count,block)=>count+block.value.records.length,0),
    totalBytes:bytes.length-4*blocks.length,streamSha256:digest(bytes),...descriptor,
  }};
}
function changed(input,mutate) {
  const copy={...input,bytes:input.bytes.slice(),descriptor:{...input.descriptor}};
  mutate(copy);
  copy.descriptor.streamSha256=digest(copy.bytes);
  return copy;
}
async function rejected(input,category) {
  const result=await verify(input);
  assert.equal(result.passed,false);
  assert.equal(result.category,category);
}
(async()=>{
  const cursorFixture=read('cursor-trajectory-v1.json');
  const cursor=request('cursor',[cursorFixture.canonicalVectors[0]]);
  assert.equal(cursor.descriptor.streamSha256,cursorFixture.canonicalVectors[0].streamSha256);
  const eventFixture=read('recording-events-v1.json');
  const events=request('events',eventFixture.blocks,eventFixture.descriptor);
  for(const input of [cursor,events]) {
    const result=await verify(input);
    assert.equal(result.passed,true);
    assert.equal(result.blocks,input.descriptor.totalBlocks);
    assert.equal(result.records,input.descriptor.totalRecords);
    assert.equal(result.bytes,input.descriptor.totalBytes);
    const padded=new Uint8Array(input.bytes.length+8);
    padded.set(input.bytes,4);
    assert.equal((await verify({...input,bytes:padded.subarray(4,-4)})).passed,true);
    await rejected({...input,sourceRecordingId:'wrong-recording'},'native-history-frame-binding-invalid');
    await rejected({...input,descriptor:{...input.descriptor,finalized:false}},'native-history-fixture-descriptor-invalid');
    await rejected({...input,descriptor:{...input.descriptor,totalRecords:input.descriptor.totalRecords+1}},'native-history-totals-mismatch');
    await rejected({...input,observe:()=>false},'native-history-fixture-fact-invalid');
    const tampered=input.bytes.slice();tampered[tampered.length-1]^=1;
    await rejected({...input,bytes:tampered},'native-history-stream-sha-mismatch');
    for(const length of [0,65537,0x7fffffff]) {
      await rejected(changed(input,copy=>new DataView(copy.bytes.buffer).setUint32(0,length,true)),'native-history-frame-size-invalid');
    }
    await rejected(changed(input,copy=>copy.bytes[4]=0xff),input.kind==='cursor'?'native-history-frame-not-ascii':'native-history-frame-json-invalid');
  }
  await rejected({...cursor,descriptor:{...cursor.descriptor,totalSamples:cursor.descriptor.totalSamples+1}},'native-history-totals-mismatch');
  for(const patch of [{recordingId:'other'},{trajectoryId:'f'.repeat(32)},{sequence:1},{records:[]},
    {records:Array(513).fill({kind:'boundary',segmentId:0,activeTimeUs:0,reason:'stop'})}]) {
    const value={...cursorFixture.canonicalVectors[0].value,...patch};
    const input=request('cursor',[{value,utf8:JSON.stringify(value)}]);
    input.sourceRecordingId=cursor.sourceRecordingId;
    input.descriptor.trajectoryId=cursor.descriptor.trajectoryId;
    await rejected(input,'native-history-frame-binding-invalid');
  }
  const duplicate=request('cursor',[cursorFixture.canonicalVectors[0],cursorFixture.canonicalVectors[0]]);
  await rejected(duplicate,'native-history-frame-binding-invalid');
  const partial=changed(cursor,copy=>{
    copy.bytes=new Uint8Array(Buffer.concat([copy.bytes,Buffer.from([1,2,3])]));
    copy.descriptor.totalBlocks=2;copy.descriptor.totalBytes=copy.bytes.length-8;
  });
  await rejected(partial,'native-history-frame-header-truncated');
  const bodyOnly=changed(cursor,copy=>{
    copy.bytes=new Uint8Array([2,0,0,0]);copy.descriptor.totalBytes=0;
  });
  await rejected(bodyOnly,'native-history-frame-size-invalid');
  for(const text of ['\uFEFF'+eventFixture.blocks[0].utf8,eventFixture.blocks[0].utf8+'{}']) {
    const input=request('events',[{value:eventFixture.blocks[0].value,utf8:text}]);
    await rejected(input,text.startsWith('\uFEFF')?'native-history-frame-bom':'native-history-frame-json-invalid');
  }
  process.stdout.write('Framed native history contracts passed\n');
})().catch(error=>{process.stderr.write(`${error.stack}\n`);process.exitCode=1;});
