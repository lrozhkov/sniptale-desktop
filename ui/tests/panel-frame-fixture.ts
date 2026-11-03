// The real desktop shell runs against explicit presentation and capture responses.
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import { emit } from '@tauri-apps/api/event';
import { FixtureCaptureClient } from '../src/sandbox/fixture-client';
import type { PanelFrameRequest } from '../src/client/panel-frame';
const controls = {dragCalls:0, above:false, frames:[] as PanelFrameRequest[], settingsOpened:false, closed:false,
  delayCommand:false, releaseCommand:undefined as undefined | (() => void)};
Object.assign(window, {isTauri:true, panelFixture:controls});
history.replaceState(null, '', '/');
const capture = new FixtureCaptureClient();
await capture.subscribe((next) => {
  emit('desktop-capture-state', next).catch(() => { document.body.textContent = 'Panel fixture event failed'; });
}, () => {});
mockIPC(async (command, args) => {
  const payload = args as Record<string,unknown>;
  if (command === 'desktop_capture_snapshot') return capture.snapshot();
  if (command === 'desktop_capture_command') {
    if (controls.delayCommand) await new Promise<void>((resolve) => { controls.releaseCommand = resolve; });
    return capture.command(payload.request as Parameters<typeof capture.command>[0]);
  }
  if (command === 'desktop_preferences_snapshot') return {theme:'dark', locale:'en-US'};
  if (command === 'desktop_panel_frame') {
    const request = payload.request as PanelFrameRequest;
    controls.frames.push(structuredClone(request));
    return {left:12, top:controls.above ? (request.overlay || request.modal ? 332 : request.feedbackHeight > 0 ? request.feedbackHeight+24 : 12) : 12,
      feedbackAbove:controls.above && request.feedbackHeight > 0};
  }
  if (command === 'desktop_open_settings') { controls.settingsOpened = true; return; }
  if (command === 'desktop_close_panel') { controls.closed = true; return; }
  if (command === 'desktop_drag_panel') { controls.dragCalls += 1; return; }
  throw new Error('Unexpected panel fixture command');
}, {shouldMockEvents:true});
mockWindows('capture-panel');
await import('../src/main');
