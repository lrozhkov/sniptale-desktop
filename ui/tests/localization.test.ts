import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { FluentBundle, FluentResource } from '@fluent/bundle';
import { createLocalizer, negotiateLocale } from '../src/localization/formatter.ts';

const catalogs = {
  'en-US': readFileSync(new URL('../../resources/locales/en-US/desktop.ftl', import.meta.url), 'utf8'),
  ru: readFileSync(new URL('../../resources/locales/ru/desktop.ftl', import.meta.url), 'utf8'),
};
await test('the actual Fluent parser accepts both shipped catalogs and all web messages have Russian translations', () => {
  const keys = (text: string) => Array.from(text.matchAll(/^(web-[a-z0-9-]+)\s*=/gm), (match) => match[1]).sort();
  assert.deepEqual(keys(catalogs['en-US']), keys(catalogs.ru));
  for (const locale of ['en-US', 'ru'] as const) {
    const resource = new FluentResource(catalogs[locale]);
    const bundle = new FluentBundle(locale);
    assert.deepEqual(bundle.addResource(resource), []);
    const localizer = createLocalizer(catalogs, locale);
    for (const key of keys(catalogs[locale])) assert.notEqual(localizer.t(key), key);
  }
});
await test('negotiation and missing translations preserve an independent English fallback', () => {
  assert.equal(negotiateLocale(['ru-KZ']), 'ru');
  assert.equal(negotiateLocale(['unsupported', 'de-DE']), 'en-US');
  const localizer = createLocalizer({ 'en-US': 'hello = Hello\nonly-en = Fallback', ru: 'hello = Привет' }, 'ru');
  assert.equal(localizer.t('hello'), 'Привет');
  assert.equal(localizer.t('only-en'), 'Fallback');
});
await test('Fluent formats complete parameterized plural messages and expanded stress text', () => {
  const source = 'count = { $count ->\n [one] Один\n [few] Несколько\n *[many] Много\n}\n';
  const localizer = createLocalizer({ 'en-US': 'count = { $count } items', ru: source }, 'ru');
  assert.equal(localizer.t('count', { count: 1 }), 'Один');
  assert.equal(localizer.t('count', { count: 2 }), 'Несколько');
  assert.equal(localizer.t('count', { count: 11 }), 'Много');
  assert.equal(createLocalizer({ 'en-US': 'name = Capture', ru: '' }, 'en-US', true).t('name'), '⟦ Capture · Capture ⟧');
});

await test('malformed and duplicate English fail closed while malformed Russian falls back', () => {
  for (const invalid of ['not fluent syntax', 'hello = {', 'hello = First\nhello = Second']) {
    assert.throws(() => createLocalizer({ 'en-US': invalid, ru: 'hello = Привет' }, 'ru'));
  }
  assert.equal(createLocalizer({ 'en-US': 'hello = Hello', ru: 'hello = {' }, 'ru').t('hello'), 'Hello');
});
