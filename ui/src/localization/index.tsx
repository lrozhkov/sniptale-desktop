import { useMemo } from 'react';
import type { ReactNode } from 'react';
import english from '../../../resources/locales/en-US/desktop.ftl?raw';
import russian from '../../../resources/locales/ru/desktop.ftl?raw';
import { createLocalizer } from './formatter';
import type { Locale } from './formatter';
import { TextContext } from './context';
export type { Locale } from './formatter';

const resources = { 'en-US': english, ru: russian };
export function Localization({ locale, expanded = false, children }: { locale: Locale; expanded?: boolean; children: ReactNode }) {
  const value = useMemo(() => createLocalizer(resources, locale, expanded), [locale, expanded]);
  return <TextContext.Provider value={value}>{children}</TextContext.Provider>;
}
