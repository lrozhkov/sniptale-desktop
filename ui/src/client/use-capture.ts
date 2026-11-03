import { useCallback, useEffect, useRef, useState } from 'react';
import type { CaptureClient, CaptureRequest, CaptureSnapshot } from './types';

export function useCapture(client: CaptureClient) {
  const [snapshot, setSnapshot] = useState<CaptureSnapshot | null>(null);
  const [connectionFailed, setConnectionFailed] = useState(false);
  const [commandFailure, setCommandFailure] = useState<'busy' | 'rejected' | 'unavailable' | 'source-unavailable' | null>(null);
  const [attempt, setAttempt] = useState(0);
  const pending = useRef(false);
  const [sending, setSending] = useState(false);
  const accept = useCallback((next: CaptureSnapshot) => {
    setSnapshot((current) => current && current.revision > next.revision ? current : next);
  }, []);
  useEffect(() => {
    let active = true;
    let unsubscribe: (() => void) | undefined;
    async function connect() {
      const stop = await client.subscribe((next) => {
        if (active) { accept(next); setConnectionFailed(false); }
      }, () => { if (active) setConnectionFailed(true); });
      if (!active) { stop(); return; }
      unsubscribe = stop;
      const next = await client.snapshot();
      if (active) { accept(next); setConnectionFailed(false); }
    }
    void connect().catch(() => { if (active) setConnectionFailed(true); });
    return () => { active = false; unsubscribe?.(); };
  }, [client, attempt, accept]);
  useEffect(() => { setCommandFailure(null); }, [snapshot?.operationEpoch, snapshot?.outcome]);
  const send = useCallback((request: CaptureRequest): void => {
    if (pending.current) return;
    pending.current = true; setSending(true); setCommandFailure(null);
    async function dispatch() {
      try {
        await client.command(request);
        try { accept(await client.snapshot()); }
        catch { setConnectionFailed(true); }
      } catch (error) {
        setCommandFailure(error === 'busy' || error === 'rejected' || error === 'source-unavailable' ? error : 'unavailable');
        try { accept(await client.snapshot()); }
        catch { setConnectionFailed(true); }
      } finally {
        pending.current = false; setSending(false);
      }
    }
    dispatch().catch(() => { pending.current = false; setSending(false); setConnectionFailed(true); });
  }, [client, accept]);
  const resetCommandFailure = useCallback(() => setCommandFailure(null), []);
  return { resetCommandFailure, snapshot, connectionFailed, commandFailure, sending, send, reconnect:() => setAttempt((value) => value + 1) };
}
