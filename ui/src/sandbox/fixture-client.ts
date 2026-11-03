import type { Availability, CaptureClient, CaptureRequest, CaptureSnapshot } from '../client/types';

export type FixtureScenario = 'success' | 'permission-denied' | 'save-failed' | 'unsupported' | 'pending' | 'setup-required' | 'command-refused' | 'connection-failed' | 'window-lost';
const available: Availability = { supported: true, permission: 'granted', sourceAvailable: true };
const initial = (): CaptureSnapshot => ({
  revision: 0, operationEpoch: 0, defaultQuality: 'balanced', setupRequired:false, phase: 'idle', kind: 'screenshot', elapsedMs: 0, commands: ['start'], outcome: null,
  capabilities: { screenshot: { ...available }, video: { ...available }, targets: {
    region: { ...available }, display: { ...available }, window: { ...available },
  } },
});

/** Explicit demo responses. These scripts never capture, publish, or model backend policy. */
export class FixtureCaptureClient implements CaptureClient {
  private state = initial();
  private listeners = new Set<(state: CaptureSnapshot) => void>();
  private failures = new Set<() => void>();
  private timers: number[] = [];
  scenario: FixtureScenario = 'success';
  snapshot = async () => structuredClone(this.state);
  subscribe = async (listener: (state: CaptureSnapshot) => void, onFailure: () => void) => {
    this.listeners.add(listener); this.failures.add(onFailure);
    return () => { this.listeners.delete(listener); this.failures.delete(onFailure); };
  };
  reset(scenario: FixtureScenario) {
    this.timers.forEach(window.clearTimeout);
    this.timers = [];
    this.scenario = scenario;
    const fresh = initial();
    fresh.operationEpoch = this.state.operationEpoch + 1;
    if (scenario === 'setup-required') { fresh.setupRequired = true; fresh.commands = []; }
    if (scenario === 'unsupported') {
      fresh.setupRequired = true; // Unsupported capability takes precedence over missing setup.
      fresh.commands = [];
      fresh.capabilities.screenshot.supported = false;
      fresh.capabilities.video.supported = false;
    }
    this.emit(fresh);
    if (scenario === 'connection-failed') this.failures.forEach((fail) => fail());
  }
  private emit(patch: Partial<CaptureSnapshot>) {
    this.state = { ...this.state, ...patch, revision: this.state.revision + 1 };
    this.listeners.forEach((listener) => listener(structuredClone(this.state)));
  }
  private later(delay: number, patch: Partial<CaptureSnapshot>) {
    this.timers.push(window.setTimeout(() => this.emit(patch), delay));
  }
  command = async (request: CaptureRequest) => {
    if (request.expectedEpoch !== this.state.operationEpoch) throw new Error('Stale fixture action');
    if (this.scenario === 'command-refused') throw 'busy';
    if (this.scenario === 'window-lost' && request.command === 'start' && request.options.target === 'window') {
      this.emit({capabilities:{...this.state.capabilities, targets:{...this.state.capabilities.targets, window:{...available, sourceAvailable:false}}}});
      throw 'source-unavailable';
    }
    const { command } = request;
    if (request.command === 'start') {
      const { options } = request;
      this.emit({ operationEpoch: this.state.operationEpoch + 1, phase: options.target === 'region' ? 'selecting' : 'starting', kind: options.kind, commands: [], outcome: null });
      if (this.scenario === 'pending') return;
      if (this.scenario === 'permission-denied') {
        this.later(700, { phase: 'failed', commands: ['start'], outcome: 'permission-denied' });
      } else if (options.kind === 'video') {
        this.later(800, { phase: 'recording', commands: ['pause', 'stop', 'discard'], elapsedMs: 12500 });
      } else {
        this.later(700, { phase: 'saving', commands: [] });
        this.later(1400, this.scenario === 'save-failed'
          ? { phase: 'failed', commands: ['start'], outcome: 'save-failed' }
          : { phase: 'complete', commands: ['start'], outcome: 'saved' });
      }
    } else if (command === 'pause') {
      this.emit({ phase: 'paused', commands: ['resume', 'stop', 'discard'] });
    } else if (command === 'resume') {
      this.emit({ phase: 'recording', commands: ['pause', 'stop', 'discard'] });
    } else if (command === 'discard') {
      this.emit({ phase: 'complete', commands: ['start'], outcome: 'discarded' });
    } else {
      this.emit({ phase: 'saving', commands: [] });
      this.later(800, this.scenario === 'save-failed'
        ? { phase: 'failed', commands: ['start'], outcome: 'save-failed' }
        : { phase: 'complete', commands: ['start'], outcome: 'saved' });
    }
  };
}
