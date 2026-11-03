import { parsePanelInsets } from './panel-frame';
import type { PanelFrameClient } from './panel-frame';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { encodeSettingsRequest, parseSettingsSnapshot } from './settings';
import type { SettingsClient } from './settings';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { parsePreferences } from './preferences';
import type { PreferencesClient } from './preferences';
import { parseCaptureSnapshot } from './validation';
import type { CaptureClient } from './types';

// Components use typed clients. Only this adapter imports the Tauri transport.
export const tauriCaptureClient: CaptureClient = {
  snapshot: async () => parseCaptureSnapshot(await invoke<unknown>('desktop_capture_snapshot')),
  subscribe: async (listener, onFailure) => {
    return listen<unknown>('desktop-capture-state', (event) => {
      try { listener(parseCaptureSnapshot(event.payload)); } catch { onFailure(); }
    });
  },
  command: (request) => invoke('desktop_capture_command', { request }),
};

export const isDesktopRuntime = () => isTauri();
export const openDesktopSettings = () => invoke<void>('desktop_open_settings');
export const closeDesktopPanel = () => invoke<void>('desktop_close_panel');
export const dragDesktopPanel = () => invoke<void>('desktop_drag_panel');
export const tauriPreferencesClient: PreferencesClient = {
  snapshot: async () => parsePreferences(await invoke<unknown>('desktop_preferences_snapshot')),
  save: (patch) => invoke('desktop_preferences_save', { patch }),
  subscribe: (listener, onFailure) => listen<unknown>('desktop-preferences', (event) => {
    try { listener(parsePreferences(event.payload)); } catch { onFailure(); }
  }),
};

export const tauriSettingsClient: SettingsClient = {
  request: async (request) => parseSettingsSnapshot(await invoke<unknown>('desktop_settings_command', { request: encodeSettingsRequest(request) })),
  onCloseRequested: (listener) => getCurrentWindow().onCloseRequested((event) => { event.preventDefault(); listener(); }),
  close: () => invoke<void>('desktop_close_settings'),
};

export const tauriPanelFrameClient: PanelFrameClient = {
  layout: async (request) => parsePanelInsets(await invoke<unknown>('desktop_panel_frame', {request})),
  drag: dragDesktopPanel,
};

export const tauriPromptClient: import('./prompts').PromptClient = {
  snapshot: async () => (await import('./prompts')).parsePromptView(await invoke<unknown>('desktop_prompt_snapshot')),
  reply: (generation, choice) => invoke<void>('desktop_prompt_reply', {reply:{generation, choice}}),
  dismiss: () => invoke<void>('desktop_prompt_dismiss'),
};
