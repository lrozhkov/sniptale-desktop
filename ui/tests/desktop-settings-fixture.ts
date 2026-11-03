// Browser transport proof. These fixtures do not access operating-system settings.
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';

const controls = { failure: '', cancelFolder: false, closed: false, writes: 0, deliveryWrites:0, blockPreferences:true, preferenceWrites:[] as unknown[], releasePreferences:() => {} };
Object.assign(window, { isTauri: true, settingsFixture: controls });
history.replaceState(null, '', '/settings');
let preferences = { theme:'system', locale:'en-US' };
const snapshot = {
  profileRevision:1, selectedFolder:null as string | null,
  delivery:{targets:[{id:'browser-one', label:'Chrome — Personal'}], targetId:null as string | null, askAfterCapture:false},
  draft:{destinationPath:'C:\\Captures', screenshotIncludeCursor:true, videoQuality:'high', videoFps:'fps30',
    videoBitrateOverrideMbps:null, videoHardwarePreference:'prefer', videoIncludeCursor:true,
    collectPointerEvents:false, videoMaxDurationSeconds:3600, audioSourceMode:'mixed', microphoneDeviceId:null,
    audioBitrateKbps:128, hotkeys:{'screenshot-region':'Ctrl+Shift+S'}},
  microphones:[{id:null, name:'', missing:false, available:true}],
  audioSources:[{source:'system', available:true}, {source:'microphone', available:true}, {source:'mixed', available:true}],
};
mockIPC(async (command, args) => {
  const payload = args as Record<string, unknown> | undefined;
  if (!payload) throw new Error('Fixture arguments missing');
  if (command === 'desktop_preferences_snapshot') return structuredClone(preferences);
  if (command === 'desktop_preferences_save') {
    controls.preferenceWrites.push(payload.patch);
    if (controls.blockPreferences && controls.preferenceWrites.length === 1) await new Promise<void>((resolve) => { controls.releasePreferences = resolve; });
    preferences = {...preferences, ...payload.patch as object};
    await emit('desktop-preferences', preferences);
    return;
  }
  if (command === 'desktop_close_settings') { controls.closed = true; return; }
  if (command !== 'desktop_settings_command') throw new Error('Unexpected fixture command');
  const request = payload.request as {command: string; draft: typeof snapshot.draft; expectedRevision: number; targetId:string | null; askAfterCapture:boolean};
  if (request.command === 'choose-folder') {
    return {...structuredClone(snapshot), selectedFolder: controls.cancelFolder ? null : 'D:\\Captures'};
  }
  if (request.command === 'save-delivery') {
    if (controls.failure) throw controls.failure;
    snapshot.delivery.targetId = request.targetId; snapshot.delivery.askAfterCapture = request.askAfterCapture; controls.deliveryWrites++;
  }
  if (request.command === 'apply') {
    if (controls.failure) throw controls.failure;
    if (request.expectedRevision !== snapshot.profileRevision) throw 'stale-settings';
    snapshot.draft = structuredClone(request.draft); snapshot.profileRevision++; controls.writes++;
  }
  return structuredClone(snapshot);
}, {shouldMockEvents:true});
mockWindows('application-settings');
await import('../src/main');
