import type { Locale } from '../localization/formatter';
import { localeDescriptors } from '../localization/formatter';
export type Theme = 'system' | 'light' | 'dark';
export type UiPreferences = { theme: Theme; locale: Locale };
export type PreferencesPatch = Partial<UiPreferences>;
export interface PreferencesClient {
  snapshot(): Promise<UiPreferences>;
  save(value: PreferencesPatch): Promise<void>;
  subscribe(listener: (value: UiPreferences) => void, onFailure: () => void): Promise<() => void>;
}
export function parsePreferences(raw: unknown): UiPreferences {
  if (raw === null || typeof raw !== 'object' || Array.isArray(raw)) throw new Error('Invalid UI preferences');
  const value = raw as Record<string, unknown>;
  if (Object.keys(value).length !== 2 || !Object.hasOwn(value, 'theme') || !Object.hasOwn(value, 'locale') ||
      typeof value.theme !== 'string' || !['system', 'light', 'dark'].includes(value.theme) ||
      !localeDescriptors.some((locale) => locale.tag === value.locale)) throw new Error('Invalid UI preferences');
  return { theme: value.theme as Theme, locale: value.locale as Locale };
}

export function parsePreferencesPatch(raw: unknown): PreferencesPatch {
  if (raw === null || typeof raw !== 'object' || Array.isArray(raw)) throw new Error('Invalid preference update');
  const value = raw as Record<string, unknown>;
  if (!Object.keys(value).length || Object.keys(value).some((key) => !['theme', 'locale'].includes(key))) throw new Error('Invalid preference update fields');
  const merged = parsePreferences({ theme: 'system', locale: 'en-US', ...value });
  return { ...(Object.hasOwn(value, 'theme') ? { theme: merged.theme } : {}),
    ...(Object.hasOwn(value, 'locale') ? { locale: merged.locale } : {}) };
}
