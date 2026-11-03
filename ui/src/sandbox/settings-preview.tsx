import { useState } from 'react';
import type { DeliverySettingsPresentation } from '../components/delivery-settings';
import type { DeliverySelection } from '../client/settings';
import { AppSettings } from '../components/app-settings';
import type { CaptureSettingsDraft, CaptureSettingsPresentation } from '../components/capture-settings';
import { ChoiceMenu } from '../components/menu';
import type { UiPreferences, PreferencesPatch } from '../client/preferences';
import { useText } from '../localization/context';

const initial: CaptureSettingsDraft = {
  destinationPath: 'C:\\Captures', screenshotIncludeCursor:true, videoQuality:'high', videoFps:'30',
  videoBitrateOverrideMbps:null, videoHardwarePreference:'prefer', videoIncludeCursor:true,
  collectPointerEvents:false, videoMaxDurationSeconds:3600, audioSourceMode:'mixed', microphoneDeviceId:'', audioBitrateKbps:128,
  hotkeys:{'screenshot-region':'Ctrl+Shift+S', 'recording-start-screen':'Ctrl+Shift+R', 'recording-stop':'Ctrl+Shift+X'},
};
type Scenario = 'ready' | 'save-failed' | 'pending' | 'unavailable';
/** Explicit preview owner. No operating-system settings or global shortcuts are changed. */
export function SettingsPreview(props: { value: UiPreferences; save: (patch: PreferencesPatch) => void; failed: boolean }) {
  const { t } = useText();
  const [saved, setSaved] = useState(() => structuredClone(initial));
  const [draft, setDraft] = useState(() => structuredClone(initial));
  const [scenario, setScenario] = useState<Scenario>('ready');
  const [failure, setFailure] = useState<string | null>(null);
  const [savedDelivery, setSavedDelivery] = useState<DeliverySelection>({targetId:null, askAfterCapture:false});
  const [deliveryDraft, setDeliveryDraft] = useState<DeliverySelection>({targetId:null, askAfterCapture:false});
  const dirty = JSON.stringify(draft) !== JSON.stringify(saved);
  const model: CaptureSettingsPresentation = {
    draft, dirty, pending:scenario === 'pending', unavailable:scenario === 'unavailable', failure,
    change: (value) => { setDraft(value); setFailure(null); },
    devices:[{value:'', label:t('settings-device-default')}, {value:'preview-microphone', label:t('web-preview-microphone')}],
    chooseFolder: () => setDraft({...draft, destinationPath:draft.destinationPath === 'C:\\Captures' ? 'D:\\Captures' : 'C:\\Captures'}),
    refreshDevices: () => setFailure(null),
    apply: () => { if (!dirty) return; if (scenario === 'save-failed') { setFailure('feedback-save-failed'); return; } setSaved(structuredClone(draft)); setFailure(null); },
    cancel: () => { setDraft(structuredClone(saved)); setFailure(null); },
  };
  const delivery: DeliverySettingsPresentation = {
    delivery:{...savedDelivery, targets:[{id:'preview-browser', label:t('web-preview-browser')}]}, draft:deliveryDraft,
    dirty:JSON.stringify(deliveryDraft) !== JSON.stringify(savedDelivery), pending:scenario === 'pending', unavailable:scenario === 'unavailable', failure,
    change:(value) => { setDeliveryDraft(value); setFailure(null); },
    apply:() => { if (scenario === 'save-failed') setFailure('feedback-save-failed'); else { setSavedDelivery({...deliveryDraft}); setFailure(null); } },
    cancel:() => { setDeliveryDraft({...savedDelivery}); setFailure(null); }, refresh:() => setFailure(null),
  };
  return <><div className="settings-preview-controls"><span>{t('web-settings-preview')}</span>
    <ChoiceMenu label={t('web-scenario')} value={scenario} choices={(['ready','save-failed','pending','unavailable'] as const).map((value) => ({value, label:t(`web-settings-scenario-${value}`)}))}
      onChange={(value) => { setScenario(value as Scenario); setFailure(null); }}><span>{t(`web-settings-scenario-${scenario}`)}</span></ChoiceMenu>
  </div><AppSettings {...props} preview capture={model} delivery={delivery} /></>;
}
