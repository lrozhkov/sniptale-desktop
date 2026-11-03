import type { Availability, CaptureCommand, CaptureKind, CaptureQuality, CapturePhase, CaptureSnapshot, CaptureTarget } from './types.ts';

export function object(value: unknown): Record<string, unknown> {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) throw new Error('Invalid desktop response');
  return value as Record<string, unknown>;
}
export function keys(value: Record<string, unknown>, expected: string[]) {
  if (Object.keys(value).length !== expected.length || expected.some((key) => !Object.hasOwn(value, key))) throw new Error('Unexpected desktop response fields');
}
export function choice<const T extends string>(value: unknown, values: readonly T[]): T {
  if (typeof value !== 'string' || !values.includes(value as T)) throw new Error('Invalid desktop response value');
  return value as T;
}
export function count(value: unknown): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) throw new Error('Invalid desktop response number');
  return value;
}
function availability(value: unknown): Availability {
  const item = object(value);
  keys(item, ['supported', 'permission', 'sourceAvailable']);
  if (typeof item.supported !== 'boolean' || typeof item.sourceAvailable !== 'boolean') throw new Error('Invalid desktop capability');
  return { supported: item.supported, sourceAvailable: item.sourceAvailable,
    permission: choice(item.permission, ['granted', 'prompt', 'denied', 'not-required']) };
}
export function parseCaptureSnapshot(value: unknown): CaptureSnapshot {
  const item = object(value);
  keys(item, ['revision', 'operationEpoch', 'defaultQuality', 'setupRequired', 'phase', 'kind', 'elapsedMs', 'commands', 'capabilities', 'outcome']);
  if (!Array.isArray(item.commands) || item.commands.length > 5 || new Set(item.commands).size !== item.commands.length) throw new Error('Invalid desktop commands');
  if (typeof item.setupRequired !== 'boolean') throw new Error('Invalid capture setup state');
  const commands = item.commands.map((command) => choice<CaptureCommand>(command, ['start', 'pause', 'resume', 'stop', 'discard']));
  const capabilities = object(item.capabilities);
  keys(capabilities, ['screenshot', 'video', 'targets']);
  const targets = object(capabilities.targets);
  keys(targets, ['region', 'display', 'window']);
  const parsedTargets = Object.fromEntries(Object.entries(targets).map(([key, available]) => [key, availability(available)])) as Record<CaptureTarget, Availability>;
  return {
    setupRequired: item.setupRequired,
    revision: count(item.revision), operationEpoch: count(item.operationEpoch), defaultQuality: choice<CaptureQuality>(item.defaultQuality, ['low', 'balanced', 'high', 'ultra']), elapsedMs: count(item.elapsedMs), commands,
    kind: choice<CaptureKind>(item.kind, ['screenshot', 'video']),
    phase: choice<CapturePhase>(item.phase, ['idle', 'selecting', 'starting', 'recording', 'paused', 'saving', 'complete', 'failed']),
    outcome: item.outcome === null ? null : choice(item.outcome, ['saved', 'discarded', 'permission-denied', 'save-failed', 'capture-failed', 'cancelled']),
    capabilities: { screenshot: availability(capabilities.screenshot), video: availability(capabilities.video), targets: parsedTargets },
  };
}
