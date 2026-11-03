import { useEffect } from 'react';
import { localeDescriptor } from './localization/formatter';
import { Localization } from './localization';
import { useText } from './localization/context';
import { Sandbox } from './sandbox/app';
import { DesktopSettings } from './components/desktop-settings';
import { SettingsPreview } from './sandbox/settings-preview';
import { DesktopPanel } from './components/desktop-panel';
import { usePreferences } from './client/use-preferences';
import { closeDesktopPanel, openDesktopSettings, tauriCaptureClient, tauriSettingsClient, tauriPanelFrameClient } from './client/tauri';
import { desktop, initial, preferences } from './bootstrap';

function Desktop() {
  const { t } = useText();
  if (!desktop) return <p className="desktop-required">{t('web-desktop-required')}</p>;
  return <DesktopPanel client={tauriCaptureClient} frame={tauriPanelFrameClient} openSettings={openDesktopSettings} close={closeDesktopPanel} />;
}
export function App() {
  const { value, save, failed } = usePreferences(preferences, initial);
  const { locale, theme } = value;
  useEffect(() => {
    const media = matchMedia('(prefers-color-scheme: dark)');
    const apply = () => { document.documentElement.dataset.theme = theme === 'system' ? media.matches ? 'dark' : 'light' : theme; };
    apply(); media.addEventListener('change', apply);
    return () => media.removeEventListener('change', apply);
  }, [theme]);
  useEffect(() => { document.documentElement.lang = locale; document.documentElement.dir = localeDescriptor(locale).direction; }, [locale]);
  return <Localization locale={locale}>{location.pathname === '/settings'
    ? desktop ? <DesktopSettings value={value} save={save} failed={failed} client={tauriSettingsClient} /> : <SettingsPreview value={value} save={save} failed={failed} />
    : location.pathname === '/sandbox'
      ? <Sandbox theme={theme} setTheme={(theme) => save({ theme })} setLocale={(locale) => save({ locale })} />
      : <Desktop />}</Localization>;
}
