import { Camera, Globe2, Send, Keyboard, Mic, Monitor, Moon, Palette, Sun, Video } from 'lucide-react';
import { useState } from 'react';
import type { UiPreferences, PreferencesPatch } from '../client/preferences';
import { useText } from '../localization/context';
import { ChoiceMenu } from './menu';
import { localeDescriptor, localeDescriptors } from '../localization/formatter';
import { CaptureSettingsForm } from './capture-settings';
import type { CaptureSettingsPresentation } from './capture-settings';
import { DeliverySettings } from './delivery-settings';
import type { DeliverySettingsPresentation } from './delivery-settings';
import { SettingsRow } from './settings-controls';

export function AppSettings({ value, save, failed, preview, capture, delivery }: {
  value: UiPreferences; save: (value: PreferencesPatch) => void; failed: boolean; preview: boolean;
  capture?: CaptureSettingsPresentation; delivery?: DeliverySettingsPresentation;
}) {
  const { t } = useText();
  const [section, setSection] = useState<'general' | 'capture' | 'video' | 'audio' | 'shortcuts' | 'delivery'>('general');
  const sections = [
    { id: 'general', label: t('web-general'), icon: Palette },
    { id: 'capture', label: t('settings-capture'), icon: Camera },
    { id: 'video', label: t('settings-video'), icon: Video },
    { id: 'audio', label: t('settings-audio'), icon: Mic },
    { id: 'shortcuts', label: t('settings-shortcuts'), icon: Keyboard },
    ...(delivery ? [{ id:'delivery' as const, label:t('settings-delivery'), icon:Send }] : []),
  ] as const;
  return <main className="app-settings-window">
    <header><h1>{t('web-app-settings')}</h1><p>{t('settings-description')}</p></header>
    <div className={capture ? 'settings-workspace' : undefined}>
      {capture && <nav className="settings-navigation" aria-label={t('web-settings-sections')}>
        {sections.map(({id, label, icon: Icon}) => <button type="button" key={id} aria-current={section === id ? 'page' : undefined}
          onClick={() => setSection(id)}><Icon /><span>{label}</span></button>)}
      </nav>}
      <div className="settings-page">
        {section === 'general' ? <>
          <div className="settings-section-intro"><h2>{t('web-general')}</h2><p>{t('web-general-description')}</p></div>
          <SettingsRow label={t('web-theme')}><div className="settings-theme-switch" role="group" aria-label={t('web-theme')}>
            {([{theme:'system', icon:Monitor, key:'web-system'}, {theme:'light', icon:Sun, key:'theme-light'}, {theme:'dark', icon:Moon, key:'theme-dark'}] as const).map(({theme, icon: Icon, key}) =>
              <button type="button" className="tool-button" aria-pressed={value.theme === theme} key={theme} onClick={() => save({theme})}><Icon /><span>{t(key)}</span></button>)}
          </div></SettingsRow>
          <SettingsRow label={t('web-language')}><ChoiceMenu appearance="field" label={t('web-language')} value={value.locale}
            choices={localeDescriptors.map((locale) => ({value:locale.tag, label:locale.nativeName}))}
            onChange={(next) => { const locale = localeDescriptors.find((item) => item.tag === next); if (locale) save({locale:locale.tag}); }}>
            <Globe2 /><span>{localeDescriptor(value.locale).nativeName}</span>
          </ChoiceMenu></SettingsRow>
        </> : section === 'delivery' ? delivery && <><div className="settings-section-intro"><h2>{t('settings-delivery')}</h2></div><DeliverySettings model={delivery} /></> : capture && <>
          <div className="settings-section-intro"><h2>{sections.find((item) => item.id === section)?.label}</h2></div>
          <CaptureSettingsForm section={section} model={capture} />
        </>}
        {failed && <p role="alert" className="error">{t('web-preferences-failed')}</p>}
      </div>
    </div>
    {preview && <footer>{t('web-sandbox-note')}</footer>}
  </main>;
}
