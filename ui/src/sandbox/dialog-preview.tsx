import { useText } from '../localization/context';
import { useMemo } from 'react';
import { ApplicationDialog } from '../components/application-dialog';
import type { PromptClient, PromptContent } from '../client/prompts';

/** Explicit fixture route; packaged windows cannot navigate here. */
export function DialogPreview() {
  const {t} = useText();
  const client = useMemo<PromptClient>(() => {
    const query = new URLSearchParams(location.search);
    const kind = query.get('kind');
    const content:PromptContent = kind === 'destination' ? {kind:'destination', localSaved:false, targets:query.has('expanded') ? Array.from({length:16}, (_, i) => `Browser ${i + 1} — ${'Long destination name '.repeat(3)}`) : ['Chrome', 'Firefox']}
      : kind === 'quit-recording' ? {kind:'quit-recording'}
        : {kind:'connection', extension:'Sniptale extension', launcher:'Google Chrome', category:'browser'};
    return {snapshot:async () => { if (query.has('failure')) throw new Error('Fixture unavailable'); return {generation:1, content}; },
      reply:async (_, choice) => { document.documentElement.dataset.promptChoice = String(choice); },
      dismiss:async () => { document.documentElement.dataset.promptChoice = '0'; }};
  }, []);
  return <><span className="dialog-preview-label">{t('web-preview')}</span><ApplicationDialog client={client} /></>;
}
