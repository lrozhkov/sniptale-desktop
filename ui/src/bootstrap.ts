import { negotiateLocale } from './localization/formatter';
import { isDesktopRuntime, tauriPreferencesClient } from './client/tauri';
import { FixturePreferencesClient } from './sandbox/preferences-client';

export const desktop = isDesktopRuntime();
if (desktop && location.pathname === '/') document.documentElement.dataset.captureWindow = 'true';
export const initial = { locale: negotiateLocale(navigator.languages), theme: 'system' as const };
export const preferences = desktop ? tauriPreferencesClient : new FixturePreferencesClient(initial, location.pathname !== '/settings');
function dispose() {
  window.removeEventListener('pagehide', dispose);
  if (preferences instanceof FixturePreferencesClient) preferences.dispose();
}
window.addEventListener('pagehide', dispose, { once: true });
if (import.meta.hot) import.meta.hot.dispose(dispose);
