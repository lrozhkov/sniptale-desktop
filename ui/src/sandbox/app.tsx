import { ArrowUpRight, Camera, RotateCcw, SlidersHorizontal, Type, Settings2 } from 'lucide-react';
import { useMemo, useRef, useState } from 'react';
import type { PointerEvent as ReactPointerEvent } from 'react';
import { CaptureToolbar } from '../components/capture-toolbar';
import { Preferences } from '../components/preferences';
import type { Theme } from '../components/preferences';
import { Localization } from '../localization';
import { useText } from '../localization/context';
import type { Locale } from '../localization';
import { ChoiceMenu } from '../components/menu';
import { FixtureCaptureClient } from './fixture-client';
import type { FixtureScenario } from './fixture-client';

export function Sandbox({ theme, setTheme, setLocale }: {
  theme: Theme; setTheme: (theme: Theme) => void; setLocale: (locale: Locale) => void;
}) {
  const { t, locale } = useText();
  const client = useMemo(() => new FixtureCaptureClient(), []);
  const [scenario, setScenario] = useState<FixtureScenario>('success');
  const [visible, setVisible] = useState(true);
  const [stress, setStress] = useState('normal');
  const [scale, setScale] = useState('100');
  const [offset, setOffset] = useState({ x: 0, y: 0 });
  const canvas = useRef<HTMLDivElement>(null);
  function drag(event: ReactPointerEvent<HTMLButtonElement>) {
    if (event.button !== 0) return;
    event.preventDefault();
    const handle = event.currentTarget;
    handle.setPointerCapture(event.pointerId);
    const initial = { ...offset };
    const originX = event.clientX, originY = event.clientY;
    const move = (next: PointerEvent) => {
      const bounds = canvas.current?.getBoundingClientRect();
      const zoom = Number(scale) / 100;
      const toolbarWidth = canvas.current?.querySelector('[role="toolbar"]')?.getBoundingClientRect().width ?? 600;
      const limitX = Math.max(0, ((bounds?.width ?? 720) - toolbarWidth - 24) / (2 * zoom));
      setOffset({ x: Math.max(-limitX, Math.min(limitX, initial.x + (next.clientX - originX) / zoom)),
        y: Math.max(-80, Math.min(100, initial.y + (next.clientY - originY) / zoom)) });
    };
    const end = () => {
      handle.removeEventListener('pointermove', move);
      handle.removeEventListener('lostpointercapture', end);
      handle.removeEventListener('pointerup', end);
    };
    handle.addEventListener('pointermove', move);
    handle.addEventListener('lostpointercapture', end);
    handle.addEventListener('pointerup', end, { once: true });
  }
  return <main className="sandbox-shell">
    <header className="sandbox-header"><a className="brand" href="/sandbox"><span className="brand-mark">S</span><span>Sniptale</span></a>
      <span className="header-divider" /><span className="header-caption">{t('web-design-system')}</span>
      <Preferences theme={theme} setTheme={setTheme} setLocale={setLocale} />
    </header>
    <div className="sandbox-layout">
      <nav className="sandbox-navigation" aria-label={t('web-sandbox')}>
        <a href="#capture" aria-current="page"><Camera /><span>{t('web-capture-toolbar')}</span></a>
        <a href="/settings?preview=1" target="sniptale-app-settings"><Settings2 /><span>{t('web-app-settings')}</span></a>
        <p>{t('web-sandbox-note')}</p>
      </nav>
      <section className="sandbox-content" id="capture">
        <div className="section-intro"><div><h1>{t('web-toolbar-title')}</h1><p>{t('web-toolbar-description')}</p></div>
          <span className="platform-label">Tauri / Web</span></div>
        <div className="preview-controls">
          <span className="preview-label">{t('web-preview')}</span>
          <ChoiceMenu label={t('web-text-preview')} value={stress} onChange={setStress} choices={[
            { value: 'normal', label: t('web-text-normal') }, { value: 'expanded', label: t('web-text-expanded') },
            { value: 'rtl', label: t('web-text-rtl') }, { value: 'expanded-rtl', label: t('web-text-expanded-rtl') },
          ]}><Type /></ChoiceMenu>
          <ChoiceMenu label={t('web-text-scale')} value={scale} onChange={(value) => { setScale(value); setOffset({ x: 0, y: 0 }); }}
            choices={['100', '150', '200'].map((value) => ({ value, label: `${value}%` }))}>{scale}%</ChoiceMenu>
          <ChoiceMenu label={t('web-scenario')} value={scenario} onChange={(value) => {
            const selected = value as FixtureScenario;
            setScenario(selected); client.reset(selected); setVisible(true);
          }} choices={(['success', 'pending', 'permission-denied', 'save-failed', 'unsupported', 'setup-required', 'command-refused', 'connection-failed', 'window-lost'] as const).map((value) => ({ value, label: t(`web-scenario-${value}`) }))}>
            <SlidersHorizontal /><span>{t(`web-scenario-${scenario}`)}</span>
          </ChoiceMenu>
          <button className="tool-button" onClick={() => { client.reset(scenario); setVisible(true); setOffset({ x: 0, y: 0 }); }} title={t('web-reset')} aria-label={t('web-reset')}><RotateCcw /></button>
        </div>
        <div className="preview-canvas" ref={canvas}>
          {visible ? <div className="floating-preview" dir={stress.includes('rtl') ? 'rtl' : 'ltr'} style={{ translate: `${offset.x}px ${offset.y}px`, zoom: Number(scale) / 100 }}>
            <Localization locale={locale} expanded={stress.includes('expanded')}>
            <CaptureToolbar client={client} onOpenSettings={() => { window.open('/settings?preview=1', 'sniptale-app-settings'); }} onDragStart={drag} onClose={() => setVisible(false)} />
            </Localization>
          </div> : <button className="primary-button" onClick={() => setVisible(true)}><Camera />{t('web-open-panel')}</button>}
        </div>
        <footer className="preview-footer"><span>{t('web-preview-description')}</span><ArrowUpRight aria-hidden="true" /></footer>
        <div className="architecture-note"><h2>{t('web-contract-title')}</h2><p>{t('web-contract-description')}</p></div>
      </section>
    </div>
  </main>;
}
