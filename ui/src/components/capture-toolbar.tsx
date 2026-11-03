import { Camera, Check, GripVertical, LoaderCircle, Pause, Play, Square, Settings2, Trash2, Video, X } from 'lucide-react';
import { useEffect, useId, useRef, useState } from 'react';
import type { PointerEventHandler } from 'react';
import type { CaptureClient, CaptureKind, CaptureTarget, CaptureQuality } from '../client/types';
import { useCapture } from '../client/use-capture';
import { useText } from '../localization/context';
import { useCaptureFocus } from './capture-focus';
import { CaptureOptions } from './capture-options';

export function CaptureToolbar({ client, onDragStart, onClose, onOpenSettings }: {
  client: CaptureClient; onDragStart?: PointerEventHandler<HTMLButtonElement>;
  onClose: () => void; onOpenSettings: () => void;
}) {
  const { t, locale } = useText();
  const { snapshot, sending, connectionFailed, commandFailure, resetCommandFailure, reconnect, send } = useCapture(client);
  const [kindDraft, setKind] = useState<CaptureKind | null>(null);
  const kind = kindDraft ?? snapshot?.kind ?? 'screenshot';
  const [qualityDraft, setQuality] = useState<CaptureQuality | null>(null);
  const quality = qualityDraft ?? snapshot?.defaultQuality ?? 'balanced';
  const [target, setTarget] = useState<CaptureTarget>('region');
  useEffect(() => { resetCommandFailure(); }, [target, kind, resetCommandFailure]);
  const [confirmDiscard, setConfirmDiscard] = useState<number | null>(null);
  const dialog = useRef<HTMLDialogElement>(null);
  const dialogTitle = useId();
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const items = root.current?.querySelectorAll<HTMLButtonElement>('[data-toolbar-item="true"]:not(:disabled)');
    items?.forEach((item, index) => { item.tabIndex = index === 0 ? 0 : -1; });
  }, [snapshot?.commands.join(','), kind, sending]);
  useEffect(() => {
    if (confirmDiscard !== null) dialog.current?.showModal();
    else dialog.current?.close();
  }, [confirmDiscard]);
  useEffect(() => {
    if (confirmDiscard !== null && snapshot && confirmDiscard !== snapshot.operationEpoch) setConfirmDiscard(null);
  }, [confirmDiscard, snapshot]);
  const markFocus = useCaptureFocus(root, snapshot?.phase);
  const command = (action: 'start' | 'pause' | 'resume' | 'stop' | 'discard', epoch = snapshot?.operationEpoch) => {
    if (epoch === undefined) return;
    markFocus();
    send(action === 'start' ? { command: action, expectedEpoch: epoch, options: kind === 'video' ? { kind, target, quality } : { kind, target } } : { command: action, expectedEpoch: epoch });
  };
  const targetAvailable = !!snapshot?.capabilities.targets[target].supported && !!snapshot?.capabilities.targets[target].sourceAvailable && snapshot?.capabilities.targets[target].permission !== 'denied';
  const allowed = (action: string) => (action !== 'start' || targetAvailable) && !sending && !!snapshot?.commands.some((item) => item === action);
  const locked = sending || !snapshot || !snapshot.commands.includes('start');
  const busy = snapshot && ['selecting', 'starting', 'saving'].includes(snapshot.phase);
  const recording = snapshot?.phase === 'recording' || snapshot?.phase === 'paused';
  const feedback = connectionFailed ? { key: 'web-connection-failed', error: true }
    : snapshot && !snapshot.capabilities[kind].supported ? {key:'web-not-supported', error:false}
    : snapshot?.setupRequired ? {key:'web-setup-required', error:false}
    : commandFailure ? {key:`web-command-${commandFailure}`, error:true}
    : snapshot && !targetAvailable && snapshot.capabilities[kind].sourceAvailable ? {key:target === 'window' ? 'web-window-unavailable' : 'web-capture-unavailable', error:false}
    : snapshot?.outcome && !sending ? { key: `web-outcome-${snapshot.outcome}`, error: snapshot.phase === 'failed' }
    : snapshot && !snapshot.capabilities[kind].sourceAvailable ? {key:'web-capture-unavailable', error:false} : null;
  const time = new Intl.NumberFormat(locale, { minimumIntegerDigits: 2, useGrouping: false });
  const elapsed = Math.floor((snapshot?.elapsedMs ?? 0) / 1000);
  return <div className="capture-surface">
    <div className="capture-toolbar" role="toolbar" aria-busy={sending || !!busy || (!snapshot && !connectionFailed)} aria-label={t('web-capture-toolbar')} ref={root}
      onFocus={(event) => {
        const selected = (event.target as HTMLElement).closest<HTMLButtonElement>('[data-toolbar-item="true"]');
        if (selected) root.current?.querySelectorAll<HTMLButtonElement>('[data-toolbar-item="true"]').forEach((item) => { item.tabIndex = item === selected ? 0 : -1; });
      }}
      onKeyDown={(event) => {
        if ((event.target as HTMLElement).closest('[data-floating-surface]') || !['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
        event.preventDefault();
        const items = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('[data-toolbar-item="true"]:not(:disabled)'));
        const current = items.indexOf(document.activeElement as HTMLButtonElement);
        const rtl = getComputedStyle(event.currentTarget).direction === 'rtl';
        const delta = event.key === 'ArrowRight' !== rtl ? 1 : -1;
        const next = event.key === 'Home' ? 0 : event.key === 'End' ? items.length - 1 : (current + delta + items.length) % items.length;
        items[next]?.focus();
      }}>
      <button className="drag-handle" tabIndex={-1} aria-label={t('web-move-panel')} title={t('web-move-panel')} onPointerDown={onDragStart}><GripVertical /></button>
      {!recording && <>
        <button className="capture-mode-switch" data-toolbar-item="true"
          aria-label={t(kind === 'screenshot' ? 'web-switch-video' : 'web-switch-screenshot')}
          title={t(kind === 'screenshot' ? 'web-switch-video' : 'web-switch-screenshot')}
          disabled={locked || !snapshot?.capabilities[kind === 'screenshot' ? 'video' : 'screenshot'].supported}
          onClick={() => setKind(kind === 'screenshot' ? 'video' : 'screenshot')}>
          <span data-active={kind === 'screenshot'} aria-hidden="true"><Camera /></span>
          <span data-active={kind === 'video'} aria-hidden="true"><Video /></span>
        </button>
        <span className="toolbar-divider" aria-hidden="true" />
        <CaptureOptions kind={kind} target={target} quality={quality} setTarget={setTarget} setQuality={setQuality} snapshot={snapshot} disabled={locked} />
        <button className="primary-button" data-action="start" data-toolbar-item="true" disabled={!allowed('start')} onClick={() => command('start')}>
          {busy || sending ? <LoaderCircle className="spinner" /> : kind === 'screenshot' ? <Camera /> : <Video />}
          <span>{busy || sending ? t(`web-phase-${busy && snapshot ? snapshot.phase : 'starting'}`) : t(kind === 'screenshot' ? 'panel-capture' : 'panel-record')}</span>
        </button>
      </>}
      {recording && <>
        <div className="recording-clock" role="status"><span className={snapshot.phase === 'paused' ? 'paused-dot' : 'recording-dot'} />
          <span>{time.format(Math.floor(elapsed / 60))}:{time.format(elapsed % 60)}</span>
          {snapshot.phase === 'paused' && <small>{t('web-paused')}</small>}
        </div>
        <button className="tool-button" data-action={snapshot.phase === 'paused' ? 'resume' : 'pause'} data-toolbar-item="true" disabled={!allowed(snapshot.phase === 'paused' ? 'resume' : 'pause')}
          onClick={() => command(snapshot.phase === 'paused' ? 'resume' : 'pause')}>
          {snapshot.phase === 'paused' ? <Play /> : <Pause />}<span>{t(snapshot.phase === 'paused' ? 'panel-resume' : 'panel-pause')}</span>
        </button>
        <button className="primary-button" data-toolbar-item="true" disabled={!allowed('stop')} onClick={() => command('stop')}><Square /><span>{t('web-stop')}</span></button>
        <button className="tool-button danger-button" data-toolbar-item="true" disabled={!allowed('discard')} aria-label={t('panel-discard')} title={t('panel-discard')} onClick={() => setConfirmDiscard(snapshot.operationEpoch)}><Trash2 /></button>
      </>}
      <span className="toolbar-divider" aria-hidden="true" />
      <button className="tool-button" data-toolbar-item="true" aria-label={t('web-app-settings')} title={t('web-app-settings')} onClick={onOpenSettings}><Settings2 /></button>
      <button className="tool-button close-button" data-toolbar-item="true" aria-label={t('web-hide-panel')} title={t('web-hide-panel')} onClick={onClose}><X /></button>
    </div>
    {feedback && <p className={`capture-feedback ${feedback.error ? 'error' : ''}`} role={feedback.error ? 'alert' : 'status'}>
      {snapshot?.phase === 'complete' && !feedback.error && <Check />}<span>{t(feedback.key)}</span>{connectionFailed && <button type="button" className="tool-button" onClick={reconnect}>{t('web-retry')}</button>}
    </p>}
    <dialog className="dialog-surface" aria-labelledby={dialogTitle} ref={dialog} onCancel={() => setConfirmDiscard(null)}>
      <h2 id={dialogTitle}>{t('web-discard-title')}</h2><p>{t('web-discard-body')}</p>
      <div className="dialog-actions"><button autoFocus className="tool-button" onClick={() => setConfirmDiscard(null)}>{t('settings-cancel')}</button>
        <button className="primary-button destructive" onClick={() => { setConfirmDiscard(null); command('discard', confirmDiscard ?? undefined); }}>{t('panel-discard')}</button></div>
    </dialog>
  </div>;
}
