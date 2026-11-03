import type { CaptureQuality } from './types';
import { object, keys, choice, count } from './validation.ts';

/** Presentation and unsaved input. Rust validates and applies the profile. */
export type CaptureSettingsDraft = {
  destinationPath: string; screenshotIncludeCursor: boolean; videoQuality: CaptureQuality; videoFps: '24' | '30' | '60';
  videoBitrateOverrideMbps: number | null; videoHardwarePreference: 'prefer' | 'software'; videoIncludeCursor: boolean;
  collectPointerEvents: boolean; videoMaxDurationSeconds: number | null; audioSourceMode: 'system' | 'microphone' | 'mixed';
  microphoneDeviceId: string; audioBitrateKbps: number | null; hotkeys: Record<string, string>;
};
export type DeliverySelection = { targetId: string | null; askAfterCapture: boolean };
export type BrowserDelivery = DeliverySelection & { targets: { id: string; label: string }[] };
export type SettingsSnapshot = {
  selectedFolder: string | null; profileRevision: number; draft: CaptureSettingsDraft;
  microphones: { id: string | null; name: string; missing: boolean; available: boolean }[];
  delivery: BrowserDelivery;
  audioSources: { source: 'system' | 'microphone' | 'mixed'; available: boolean }[];
};
export type SettingsRequest = {command: 'open' | 'choose-folder' | 'refresh-devices' | 'refresh-browsers'} |
  {command: 'apply'; expectedRevision: number; draft: CaptureSettingsDraft} |
({command:'save-delivery'} & DeliverySelection);
export interface SettingsClient {
  request(value: SettingsRequest): Promise<SettingsSnapshot>;
  onCloseRequested(listener: () => void): Promise<() => void>;
  close(): Promise<void>;
}
function text(value: unknown): string {
  if (typeof value !== 'string') throw new Error('Invalid settings response text');
  return value;
}
function flag(value: unknown): boolean {
  if (typeof value !== 'boolean') throw new Error('Invalid settings response flag');
  return value;
}
function list(value: unknown): unknown[] {
  if (!Array.isArray(value)) throw new Error('Invalid settings response list');
  return value;
}
export function parseSettingsSnapshot(raw: unknown): SettingsSnapshot {
  const value = object(raw);
  keys(value, ['profileRevision', 'selectedFolder', 'draft', 'microphones', 'audioSources', 'delivery']);
  const delivery = object(value.delivery);
  keys(delivery, ['targets', 'targetId', 'askAfterCapture']);
  const draft = object(value.draft);
  keys(draft, ['destinationPath', 'screenshotIncludeCursor', 'videoQuality', 'videoFps', 'videoBitrateOverrideMbps',
    'videoHardwarePreference', 'videoIncludeCursor', 'collectPointerEvents', 'videoMaxDurationSeconds',
    'audioSourceMode', 'microphoneDeviceId', 'audioBitrateKbps', 'hotkeys']);
  const hotkeys = Object.fromEntries(Object.entries(object(draft.hotkeys)).map(([key, value]) => [key, text(value)]));
  const quality = choice(draft.videoQuality, ['low', 'medium', 'high', 'ultra']);
  const fps = choice(draft.videoFps, ['fps24', 'fps30', 'fps60']);
  return {
    selectedFolder: value.selectedFolder === null ? null : text(value.selectedFolder),
    profileRevision: count(value.profileRevision),
    draft: {
      destinationPath: text(draft.destinationPath), screenshotIncludeCursor: flag(draft.screenshotIncludeCursor),
      videoQuality: quality === 'medium' ? 'balanced' : quality, videoFps: fps === 'fps24' ? '24' : fps === 'fps30' ? '30' : '60',
      videoBitrateOverrideMbps: draft.videoBitrateOverrideMbps === null ? null : count(draft.videoBitrateOverrideMbps),
      videoHardwarePreference: choice(draft.videoHardwarePreference, ['prefer', 'force-software']) === 'prefer' ? 'prefer' : 'software',
      videoIncludeCursor: flag(draft.videoIncludeCursor), collectPointerEvents: flag(draft.collectPointerEvents),
      videoMaxDurationSeconds: count(draft.videoMaxDurationSeconds), audioSourceMode: choice(draft.audioSourceMode, ['system', 'microphone', 'mixed']),
      microphoneDeviceId: draft.microphoneDeviceId === null ? '' : text(draft.microphoneDeviceId),
      audioBitrateKbps: count(draft.audioBitrateKbps), hotkeys,
    },
    delivery: {
      targetId: delivery.targetId === null ? null : text(delivery.targetId), askAfterCapture: flag(delivery.askAfterCapture),
      targets: list(delivery.targets).map((raw) => { const target = object(raw); keys(target, ['id', 'label']); return {id:text(target.id), label:text(target.label)}; }),
    },
    microphones: list(value.microphones).map((raw) => {
      const item = object(raw); keys(item, ['id', 'name', 'missing', 'available']);
      return {id: item.id === null ? null : text(item.id), name: text(item.name), missing: flag(item.missing), available: flag(item.available)};
    }),
    audioSources: list(value.audioSources).map((raw) => {
      const item = object(raw); keys(item, ['source', 'available']);
      return {source: choice(item.source, ['system', 'microphone', 'mixed']), available: flag(item.available)};
    }),
  };
}
export function encodeSettingsRequest(request: SettingsRequest) {
  if (request.command !== 'apply') return request;
  const draft = request.draft;
  return {...request, draft: {...draft,
    videoQuality: draft.videoQuality === 'balanced' ? 'medium' : draft.videoQuality,
    videoFps: `fps${draft.videoFps}`, videoHardwarePreference: draft.videoHardwarePreference === 'software' ? 'force-software' : 'prefer',
    microphoneDeviceId: draft.microphoneDeviceId || null,
  }};
}
