import { useEffect, useId, useRef, useState } from 'react';
import { Folder, MonitorUp, ShieldCheck, X } from 'lucide-react';
import type { PromptClient, PromptView } from '../client/prompts';
import { useText } from '../localization/context';

/** App-owned prompts share the same tokens, focus behavior and typed client in the sandbox. */
export function ApplicationDialog({client}:{client:PromptClient}) {
  const {t} = useText();
  const [view, setView] = useState<PromptView | null>(null);
  const [failed, setFailed] = useState(false);
  const [sending, setSending] = useState(false);
  const pending = useRef(false);
  const cancel = useRef<HTMLButtonElement>(null);
  const title = useId(), body = useId();
  useEffect(() => {
    let active = true;
    client.snapshot().then((value) => { if (active) { setView(value); setFailed(false); } }, () => { if (active) setFailed(true); });
    return () => { active = false; };
  }, [client]);
  useEffect(() => { if (view) cancel.current?.focus(); }, [view]);
  function dispatch(task:() => Promise<void>) {
    if (pending.current) return;
    pending.current = true; setSending(true); setFailed(false);
    task().catch(() => { setFailed(true); pending.current = false; setSending(false); });
  }
  function reply(choice:number) { if (view) dispatch(() => client.reply(view.generation, choice)); }
  function dismiss() { dispatch(() => client.dismiss()); }
  const content = view?.content;
  const kind = content?.kind ?? 'connection';
  return <main className="application-dialog-host">
    <section className="application-dialog" role="dialog" aria-modal="true" aria-labelledby={title} aria-describedby={body} aria-busy={sending}
      onKeyDown={(event) => {
        if (event.key === 'Escape') { event.preventDefault(); dismiss(); }
        if (event.key === 'Tab') {
          const buttons = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>('button:not(:disabled)'));
          const first = buttons[0], last = buttons.at(-1);
          if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
          else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
        }
      }}>
      <header className="application-dialog-header"><span className="dialog-symbol" aria-hidden="true">{kind === 'connection' ? <ShieldCheck /> : kind === 'destination' ? <Folder /> : <MonitorUp />}</span>
        <button type="button" className="tool-button close-button" aria-label={t('settings-cancel')} disabled={sending} onClick={dismiss}><X /></button>
      </header>
      <h1 id={title}>{t(`web-dialog-${kind}-title`)}</h1>
      <p id={body}>{t(content?.kind === 'destination' && !content.localSaved ? 'web-dialog-destination-retained' : `web-dialog-${kind}-body`)}</p>
      {content?.kind === 'connection' && <dl className="dialog-connection-details">
        <div><dt>{t('web-dialog-extension')}</dt><dd><bdi>{content.extension}</bdi></dd></div>
        <div><dt>{t('web-dialog-launcher')}</dt><dd><bdi>{content.launcher}</bdi><small>{t('web-dialog-launcher-diagnostic')}</small></dd></div>
      </dl>}
      {content?.kind === 'destination' && <div className="dialog-destinations">
        <button type="button" className="tool-button" disabled={sending} onClick={() => reply(1)}><Folder /><span>{t('web-dialog-local-folder')}</span></button>
        {content.targets.map((label, index) => <button type="button" className="tool-button" key={index} disabled={sending} onClick={() => reply(index + 2)}><MonitorUp /><span>{label}</span></button>)}
      </div>}
      {failed && <p className="error" role="alert">{t('web-dialog-unavailable')}</p>}
      <footer className="dialog-actions"><button type="button" ref={cancel} className="tool-button" disabled={sending} onClick={dismiss}>{t(kind === 'connection' ? 'web-dialog-deny' : kind === 'destination' ? 'web-dialog-later' : 'settings-cancel')}</button>
        {kind !== 'destination' && <button type="button" className="primary-button" disabled={!view || sending} onClick={() => reply(1)}>{t(kind === 'connection' ? 'web-dialog-allow' : 'web-dialog-stop-exit')}</button>}
      </footer>
    </section>
  </main>;
}
