import { useEffect, useRef, useState } from 'react';
import type { CSSProperties } from 'react';
import type { CaptureClient } from '../client/types';
import type { PanelFrameClient, PanelInsets } from '../client/panel-frame';
import { useText } from '../localization/context';
import { CaptureToolbar } from './capture-toolbar';

/** Presentation geometry only. Capture and recording state stay in the supplied client. */
export function DesktopPanel({client, frame, openSettings, close}: {
  client:CaptureClient; frame:PanelFrameClient; openSettings:() => Promise<void>; close:() => Promise<void>;
}) {
  const { t } = useText();
  const root = useRef<HTMLDivElement>(null);
  const insets = useRef<PanelInsets>({left:12, top:12, feedbackAbove:false});
  const first = useRef(true);
  const [layout, setLayout] = useState(insets.current);
  const [ready, setReady] = useState(false);
  const [failed, setFailed] = useState(false);
  const retry = useRef(() => {});
  const refresh = useRef(() => {});
  useEffect(() => {
    let active = true, sending = false, queued = false, animation = 0, previous = '';
    let observedFeedback: HTMLElement | null = null;
    const observer = new ResizeObserver(schedule);
    function schedule() { if (!animation) animation = requestAnimationFrame(() => {
      animation = 0;
      measure().catch(() => { if (active) { setFailed(true); setReady(true); retry.current = refresh.current; } });
    }); }
    async function measure() {
      if (!active || !root.current) return;
      if (sending) { queued = true; return; }
      const toolbar = root.current.querySelector<HTMLElement>('[role="toolbar"]');
      if (!toolbar) return;
      const feedback = root.current.querySelector<HTMLElement>('.capture-feedback');
      observer.observe(toolbar);
      if (observedFeedback !== feedback) {
        if (observedFeedback) observer.unobserve(observedFeedback);
        if (feedback) observer.observe(feedback);
        observedFeedback = feedback;
      }
      const bounds = toolbar.getBoundingClientRect();
      const request = {first:first.current, pixelRatio:devicePixelRatio, width:bounds.width, height:bounds.height, feedbackHeight:Math.min(240, feedback?.scrollHeight ?? 0),
        overlay:!!root.current.querySelector('[data-floating-surface]'), modal:!!root.current.querySelector('dialog[open]'),
        left:insets.current.left, top:insets.current.top};
      const signature = JSON.stringify(request);
      if (signature === previous) return;
      previous = signature; sending = true;
      try {
        const next = await frame.layout(request);
        if (active) { first.current = false; insets.current = next; setLayout(next); setFailed(false); setReady(true); }
      } catch {
        if (active) {
          setFailed(true); setReady(true);
          retry.current = () => { previous = ''; schedule(); };
        }
      } finally {
        sending = false;
        if (queued && active) { queued = false; schedule(); }
      }
    }
    const mutation = new MutationObserver(schedule);
    if (root.current) mutation.observe(root.current, {subtree:true, childList:true, characterData:true, attributes:true, attributeFilter:['open','aria-expanded']});
    refresh.current = () => { previous = ''; schedule(); };
    document.fonts.ready.then(() => { if (active) schedule(); }, () => { if (active) schedule(); });
    window.addEventListener('resize', schedule);
    schedule();
    return () => { active = false; window.removeEventListener('resize', schedule); observer.disconnect(); mutation.disconnect(); cancelAnimationFrame(animation); };
  }, [frame]);
  function action(task:() => Promise<void>) {
    task().then(() => setFailed(false)).catch(() => { setFailed(true); retry.current = () => action(task); });
  }
  return <div className="desktop-panel-host" ref={root} data-feedback-above={layout.feedbackAbove}
    style={{'--panel-left':`${layout.left}px`, '--panel-top':`${layout.top}px`, visibility:ready ? 'visible' : 'hidden'} as CSSProperties}>
    <CaptureToolbar client={client} onOpenSettings={() => action(openSettings)} onClose={() => action(close)}
      onDragStart={(event) => { if (event.button !== 0) return; event.preventDefault(); action(async () => { await frame.drag(); refresh.current(); }); }} />
    {failed && <p className="panel-window-failure" role="alert">{t('web-window-action-failed')} <button type="button" className="tool-button" onClick={() => retry.current()}>{t('web-retry')}</button></p>}
  </div>;
}
