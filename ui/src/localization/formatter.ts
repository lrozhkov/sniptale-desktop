import { FluentBundle, FluentResource } from '@fluent/bundle';
import type { FluentVariable } from '@fluent/bundle';
import { parse } from '@fluent/syntax';
import { negotiateLanguages } from '@fluent/langneg';

export const localeDescriptors = [
  { tag: 'en-US', nativeName: 'English', shortName: 'EN', direction: 'ltr' },
  { tag: 'ru', nativeName: 'Русский', shortName: 'RU', direction: 'ltr' },
] as const;
export type Locale = typeof localeDescriptors[number]['tag'];
export const localeDescriptor = (locale: Locale) => localeDescriptors.find((item) => item.tag === locale) ?? localeDescriptors[0];
export type MessageArgs = Record<string, FluentVariable>;
export type Localizer = { locale: Locale; t: (key: string, args?: MessageArgs) => string };
export type Catalogs = Record<Locale, string>;
export function negotiateLocale(requested: readonly string[]): Locale {
  return negotiateLanguages(requested, localeDescriptors.map((item) => item.tag), { defaultLocale: 'en-US' })[0] as Locale;
}
function bundle(locale: Locale, source: string) {
  const syntax = parse(source, { withSpans: false });
  if (syntax.body.some((entry) => entry.type === 'Junk') || !syntax.body.some((entry) => entry.type === 'Message')) {
    throw new Error('Invalid bundled translation syntax');
  }
  const result = new FluentBundle(locale);
  if (result.addResource(new FluentResource(source)).length) throw new Error('Invalid bundled translation catalog');
  return result;
}
export function createLocalizer(catalogs: Catalogs, locale: Locale, expanded = false): Localizer {
  const fallback = bundle('en-US', catalogs['en-US']);
  let selected = fallback;
  if (locale !== 'en-US') {
    try { selected = bundle(locale, catalogs[locale]); } catch { /* Keep the independent English fallback. */ }
  }
  return { locale, t(key, args) {
    for (const source of [selected, fallback]) {
      const message = source.getMessage(key);
      if (!message?.value) continue;
      const errors: Error[] = [];
      const text = source.formatPattern(message.value, args, errors);
      if (!errors.length) return expanded ? `⟦ ${text} · ${text} ⟧` : text;
    }
    return key;
  } };
}

