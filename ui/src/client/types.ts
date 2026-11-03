export type CaptureKind = 'screenshot' | 'video';
export type CaptureTarget = 'region' | 'display' | 'window';
export type CapturePhase = 'idle' | 'selecting' | 'starting' | 'recording' | 'paused' | 'saving' | 'complete' | 'failed';
export type CaptureCommand = 'start' | 'pause' | 'resume' | 'stop' | 'discard';
export type Availability = {
  supported: boolean;
  permission: 'granted' | 'prompt' | 'denied' | 'not-required';
  sourceAvailable: boolean;
};
export type CaptureSnapshot = {
  revision: number;
  operationEpoch: number;
  defaultQuality: CaptureQuality;
  setupRequired: boolean;
  phase: CapturePhase;
  kind: CaptureKind;
  elapsedMs: number;
  commands: CaptureCommand[];
  capabilities: { screenshot: Availability; video: Availability; targets: Record<CaptureTarget, Availability> };
  outcome: null | 'saved' | 'discarded' | 'permission-denied' | 'save-failed' | 'capture-failed' | 'cancelled';
};
export type CaptureQuality = 'low' | 'balanced' | 'high' | 'ultra';
export type CaptureOptions = { kind: 'screenshot'; target: CaptureTarget } | { kind: 'video'; target: CaptureTarget; quality: CaptureQuality };
export type CaptureRequest = ({ command: 'start'; options: CaptureOptions } | { command: Exclude<CaptureCommand, 'start'> }) & { expectedEpoch: number };
export interface CaptureClient {
  snapshot(): Promise<CaptureSnapshot>;
  subscribe(listener: (state: CaptureSnapshot) => void, onFailure: () => void): Promise<() => void>;
  command(request: CaptureRequest): Promise<void>;
}
