import { useEffect, useRef } from 'react';
import type { RefObject } from 'react';
import type { CapturePhase } from '../client/types';

export function useCaptureFocus(root: RefObject<HTMLDivElement | null>, phase: CapturePhase | undefined) {
  const origin = useRef<Element | null>(null);
  useEffect(() => {
    const cancel = () => { origin.current = null; };
    const key = (event: KeyboardEvent) => { if (event.key === 'Tab') cancel(); };
    document.addEventListener('pointerdown', cancel, true);
    document.addEventListener('keydown', key, true);
    window.addEventListener('blur', cancel);
    return () => {
      document.removeEventListener('pointerdown', cancel, true);
      document.removeEventListener('keydown', key, true);
      window.removeEventListener('blur', cancel);
    };
  }, []);
  useEffect(() => {
    if (!origin.current || !phase || !['recording', 'paused', 'complete', 'failed'].includes(phase)) return;
    const active = document.activeElement;
    if (active !== document.body && active !== origin.current && active?.isConnected) { origin.current = null; return; }
    const action = phase === 'paused' ? 'resume' : phase === 'recording' ? 'pause' : 'start';
    root.current?.querySelector<HTMLButtonElement>(`[data-action="${action}"]:not(:disabled)`)?.focus();
    origin.current = null;
  }, [phase, root]);
  return () => { origin.current = document.activeElement; };
}
