import { Globe2, Monitor, Moon, Sun } from 'lucide-react';
import type { Locale } from '../localization';
import { useText } from '../localization/context';
import { localeDescriptor, localeDescriptors } from '../localization/formatter';
import { ChoiceMenu } from './menu';

import type { Theme } from '../client/preferences';
export type { Theme } from '../client/preferences';
export function Preferences({ theme, setTheme, setLocale }: {
  theme: Theme; setTheme: (theme: Theme) => void; setLocale: (locale: Locale) => void;
}) {
  const { t, locale } = useText();
  const ThemeIcon = theme === 'system' ? Monitor : theme === 'dark' ? Moon : Sun;
  return <div className="preferences">
    <ChoiceMenu chevron={false} label={t('web-theme')} value={theme} onChange={(value) => setTheme(value as Theme)} choices={[
      { value: 'system', label: t('web-system'), icon: <Monitor /> },
      { value: 'light', label: t('theme-light'), icon: <Sun /> },
      { value: 'dark', label: t('theme-dark'), icon: <Moon /> },
    ]}><ThemeIcon /></ChoiceMenu>
    <ChoiceMenu chevron={false} label={t('web-language')} value={locale} onChange={(value) => setLocale(value as Locale)} choices={localeDescriptors.map((item) => ({ value: item.tag, label: item.nativeName }))}><Globe2 /><span className="language-code">{localeDescriptor(locale).shortName}</span></ChoiceMenu>
  </div>;
}
