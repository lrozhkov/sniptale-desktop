// Pure laboratory proof for the short physical-input fixture, not a product history limit.
(() => async ({bytes, descriptor, kind, sourceRecordingId, observe}) => {
  const fail = category => ({passed:false, category});
  const safe = value => Number.isSafeInteger(value) && value >= 0;
  const digest = value => typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);
  const cursor = kind === 'cursor';
  if ((!cursor && kind !== 'events') || !(bytes instanceof Uint8Array) || bytes.length > 1048576 ||
      typeof sourceRecordingId !== 'string' || !sourceRecordingId || !descriptor || descriptor.finalized !== true ||
      !['complete','incomplete'].includes(descriptor.state) || descriptor.formatVersion !== 1 ||
      descriptor.format !== (cursor ? 'cursor-trajectory-v1' : 'recording-events-v1') ||
      descriptor.algorithm !== (cursor ? 'sed-window-v1' : 'lossless-v1') ||
      descriptor.encoding !== (cursor ? 'json-ascii-int-v1' : 'json-utf8-v1') ||
      !['totalBlocks','totalRecords','totalBytes'].every(key => safe(descriptor[key])) ||
      descriptor.totalBlocks < 1 || descriptor.totalBlocks > 16 || !digest(descriptor.streamSha256) ||
      (cursor && !safe(descriptor.totalSamples))) return fail('native-history-fixture-descriptor-invalid');
  const streamId = cursor ? descriptor.trajectoryId : descriptor.eventStreamId;
  if (typeof streamId !== 'string' || !/^[0-9a-f]{32}$/.test(streamId) ||
      bytes.length !== descriptor.totalBytes + 4*descriptor.totalBlocks) return fail('native-history-fixture-size-invalid');
  const sha = async value => Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',value)),
    byte => byte.toString(16).padStart(2,'0')).join('');
  if (await sha(bytes) !== descriptor.streamSha256) return fail('native-history-stream-sha-mismatch');
  const view = new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength);
  const decoder = new TextDecoder('utf-8',{fatal:true});
  let offset=0, blocks=0, records=0, samples=0, rawBytes=0;
  while (offset < bytes.length) {
    if (bytes.length-offset < 4) return fail('native-history-frame-header-truncated');
    const length=view.getUint32(offset,true);
    offset+=4;
    if (!length || length>65536 || length>bytes.length-offset) return fail('native-history-frame-size-invalid');
    const original=bytes.subarray(offset,offset+length);
    offset+=length;
    let block;
    try {
      if (original[0]===0xef && original[1]===0xbb && original[2]===0xbf) return fail('native-history-frame-bom');
      if (cursor && original.some(byte=>byte>127)) return fail('native-history-frame-not-ascii');
      block=JSON.parse(decoder.decode(original));
    } catch {return fail('native-history-frame-json-invalid');}
    const id=cursor ? block?.trajectoryId : block?.eventStreamId;
    if (!block || typeof block!=='object' || Array.isArray(block) || block.formatVersion!==1 ||
        block.recordingId!==sourceRecordingId || id!==streamId || block.sequence!==blocks ||
        !safe(block.startTimeUs) || !safe(block.endTimeUs) || block.endTimeUs<block.startTimeUs ||
        !Array.isArray(block.records) || !block.records.length || block.records.length>512) {
      return fail('native-history-frame-binding-invalid');
    }
    for (const record of block.records) {
      if (!record || typeof record!=='object' || Array.isArray(record)) return fail('native-history-record-invalid');
      if (cursor && record.kind==='sample') samples++;
      if (observe && observe(record,block)!==true) return fail('native-history-fixture-fact-invalid');
    }
    rawBytes+=length;
    records+=block.records.length;
    blocks++;
    if (blocks>descriptor.totalBlocks) return fail('native-history-extra-block');
  }
  if (blocks!==descriptor.totalBlocks || records!==descriptor.totalRecords || rawBytes!==descriptor.totalBytes ||
      (cursor && samples!==descriptor.totalSamples)) return fail('native-history-totals-mismatch');
  return {passed:true,blocks,records,samples,bytes:rawBytes,streamSha256:descriptor.streamSha256};
})()
