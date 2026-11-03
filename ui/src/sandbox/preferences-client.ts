import type { PreferencesClient, UiPreferences, PreferencesPatch } from '../client/preferences';
import { parsePreferences, parsePreferencesPatch } from '../client/preferences';

/** In-memory preview preferences. Only the main sandbox supplies confirmed values. */
export class FixturePreferencesClient implements PreferencesClient {
  private channel = new BroadcastChannel('sniptale-sandbox-preferences');
  private listeners = new Set<(value: UiPreferences) => void>();
  constructor(private value: UiPreferences, private owner: boolean) {
    this.channel.onmessage = (event: MessageEvent<unknown>) => {
      if (!event.data || typeof event.data !== 'object' || !('type' in event.data)) return;
      const message = event.data as Record<string, unknown>;
      if (message.type === 'read' && this.owner) { this.publish(); return; }
      if ((message.type === 'update' && this.owner) || (message.type === 'snapshot' && !this.owner)) {
        try { this.accept(this.owner ? { ...this.value, ...parsePreferencesPatch(message.patch) } : parsePreferences(message.preferences)); } catch { return; }
        if (this.owner) this.publish();
      }
    };
  }
  snapshot = async () => {
    if (!this.owner) this.channel.postMessage({ type: 'read' });
    return { ...this.value };
  };
  subscribe = async (listener: (value: UiPreferences) => void) => {
    this.listeners.add(listener);
    return () => { this.listeners.delete(listener); };
  };
  save = async (preferences: PreferencesPatch) => {
    const patch = parsePreferencesPatch(preferences);
    if (this.owner) { this.accept({ ...this.value, ...patch }); this.publish(); }
    else this.channel.postMessage({ type: 'update', patch });
  };
  dispose() { this.listeners.clear(); this.channel.close(); }
  private accept(value: UiPreferences) {
    this.value = value;
    this.listeners.forEach((listener) => listener({ ...value }));
  }
  private publish() { this.channel.postMessage({ type: 'snapshot', preferences: this.value }); }
}
