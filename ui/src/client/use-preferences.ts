import { useEffect, useRef, useState } from 'react';
import type { PreferencesClient, UiPreferences, PreferencesPatch } from './preferences';

export function usePreferences(client: PreferencesClient, initial: UiPreferences) {
  const [value, setValue] = useState(initial);
  const [failed, setFailed] = useState(false);
  const writes = useRef(Promise.resolve());
  const latestWrite = useRef(0);
  useEffect(() => {
    let active = true;
    let notifications = 0;
    let unsubscribe: (() => void) | undefined;
    const accept = (next: UiPreferences) => { if (active) setValue(next); };
    async function connect() {
      const stop = await client.subscribe((next) => { notifications++; accept(next); }, () => { if (active) setFailed(true); });
      if (!active) { stop(); return; }
      unsubscribe = stop;
      const observed = notifications;
      const snapshot = await client.snapshot();
      if (notifications === observed) accept(snapshot);
    }
    connect().catch(() => { if (active) setFailed(true); });
    return () => { active = false; unsubscribe?.(); };
  }, [client]);
  function save(next: PreferencesPatch) {
    const intent = ++latestWrite.current;
    const write = () => client.save(next);
    writes.current = writes.current.then(write, write);
    writes.current.then(() => { if (intent === latestWrite.current) setFailed(false); },
      () => { if (intent === latestWrite.current) setFailed(true); });
  }
  return { value, save, failed };
}
