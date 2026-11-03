import { RefreshCw } from 'lucide-react';
import type { BrowserDelivery, DeliverySelection } from '../client/settings';
import { useText } from '../localization/context';
import { DraftCheckbox, SettingsChoice, SettingsRow, SettingsSaveBar } from './settings-controls';

export type DeliverySettingsPresentation = {
  delivery: BrowserDelivery; draft: DeliverySelection; change: (value: DeliverySelection) => void;
  dirty: boolean; pending: boolean; unavailable: boolean; failure: string | null;
  apply: () => void; cancel: () => void; refresh: () => void;
};
export function DeliverySettings({ model }: { model: DeliverySettingsPresentation }) {
  const { t } = useText();
  const disabled = model.pending || model.unavailable;
  const choices: {value:string; label:string; disabled?:boolean}[] = [{value:'', label:t('settings-local-folder')}, ...model.delivery.targets.map((target) => ({value:target.id, label:target.label}))];
  if (model.draft.targetId && !choices.some((choice) => choice.value === model.draft.targetId)) {
    choices.push({value:model.draft.targetId, label:t('web-delivery-missing'), disabled:true});
  }
  return <form onSubmit={(event) => { event.preventDefault(); if (model.dirty && !disabled) model.apply(); }}>
    <p className="settings-description">{t('web-delivery-description')}</p>
    <fieldset className="delivery-settings-fields" disabled={disabled}>
      <SettingsRow label={t('settings-delivery')}><div className="settings-inline-actions">
        <SettingsChoice label={t('settings-delivery')} value={model.draft.targetId ?? ''} choices={choices}
          onChange={(value) => model.change({...model.draft, targetId:value || null})} />
        <button type="button" className="tool-button" title={t('settings-refresh-browsers')} aria-label={t('settings-refresh-browsers')} onClick={model.refresh}><RefreshCw /></button>
      </div></SettingsRow>
      {!model.delivery.targets.length && <p className="parameter-hint">{t('web-delivery-empty')}</p>}
      <DraftCheckbox label={t('settings-ask-destination')} checked={model.draft.askAfterCapture} onChange={(askAfterCapture) => model.change({...model.draft, askAfterCapture})} />
    </fieldset>
    <SettingsSaveBar {...model} />
  </form>;
}
