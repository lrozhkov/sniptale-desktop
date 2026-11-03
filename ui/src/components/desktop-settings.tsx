import { useEffect, useRef, useState } from 'react';
import type { PreferencesPatch, UiPreferences } from '../client/preferences';
import type { CaptureSettingsDraft, SettingsClient, SettingsRequest, SettingsSnapshot, DeliverySelection } from '../client/settings';
import { useText } from '../localization/context';
import type { DeliverySettingsPresentation } from './delivery-settings';
import { AppSettings } from './app-settings';
import type { CaptureSettingsPresentation } from './capture-settings';

export function DesktopSettings(props: { value: UiPreferences; save: (patch: PreferencesPatch) => void; failed: boolean; client: SettingsClient }) {
  const { t } = useText();
  const [snapshot, setSnapshot] = useState<SettingsSnapshot | null>(null);
  const [saved, setSaved] = useState<CaptureSettingsDraft | null>(null);
  const [revision, setRevision] = useState(0);
  const [deliverySaved, setDeliverySaved] = useState<DeliverySelection>({targetId:null, askAfterCapture:false});
  const [deliveryDraft, setDeliveryDraft] = useState<DeliverySelection>({targetId:null, askAfterCapture:false});
  const deliveryDirty = JSON.stringify(deliveryDraft) !== JSON.stringify(deliverySaved);
  const [draft, setDraft] = useState<CaptureSettingsDraft | null>(null);
  const [pending, setPending] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const sending = useRef(false);
  const mounted = useRef(false);
  const [confirmClose, setConfirmClose] = useState(false);
  const dialog = useRef<HTMLDialogElement>(null);
  const closeRequest = useRef(() => {});
  const dirty = !!draft && !!saved && JSON.stringify(draft) !== JSON.stringify(saved);
  const close = () => { props.client.close().catch(report); };
  closeRequest.current = () => { if (sending.current) return; if (dirty || deliveryDirty) setConfirmClose(true); else close(); };
  useEffect(() => { if (confirmClose) dialog.current?.showModal(); else dialog.current?.close(); }, [confirmClose]);
  function accept(next: SettingsSnapshot, scope: 'all' | 'profile' | 'delivery') {
    setSnapshot(next);
    if (scope !== 'delivery') {
      setSaved(next.draft); setDraft(structuredClone(next.draft)); setRevision(next.profileRevision);
    }
    if (scope !== 'profile') {
      const selection = {targetId:next.delivery.targetId, askAfterCapture:next.delivery.askAfterCapture};
      setDeliverySaved(selection); setDeliveryDraft({...selection});
    }
  }
  function report(error: unknown) {
    const messages: Record<string, string> = {
      'stale-settings':'web-settings-stale', 'invalid-settings':'web-settings-invalid',
      'recording-busy':'feedback-recording-busy', 'hotkey-unavailable':'feedback-hotkey-unavailable',
      'microphone-unavailable':'feedback-microphone-unavailable',
    };
    setFailure(typeof error === 'string' ? messages[error] ?? 'feedback-save-failed' : 'feedback-save-failed');
  }
  async function request(value: SettingsRequest, commit: false | 'all' | 'profile' | 'delivery') {
    if (sending.current) return;
    sending.current = true; setPending(true);
    try {
      const next = await props.client.request(value);
      if (!mounted.current) return;
      if (commit) accept(next, commit);
      else {
        setSnapshot(next);
        if (next.selectedFolder !== null) setDraft((current) => current && ({...current, destinationPath: next.selectedFolder!}));
      }
      setFailure(null);
    } catch (error) { if (mounted.current) report(error); }
    finally { sending.current = false; if (mounted.current) setPending(false); }
  }
  function submit(value: SettingsRequest, commit: false | 'all' | 'profile' | 'delivery') {
    request(value, commit).catch(report);
  }
  useEffect(() => {
    mounted.current = true;
    submit({command:'open'}, 'all');
    let disposed = false;
    let stop: (() => void) | undefined;
    props.client.onCloseRequested(() => closeRequest.current()).then((unsubscribe) => {
      if (disposed) unsubscribe(); else stop = unsubscribe;
    }).catch((error: unknown) => { if (!disposed) report(error); });
    return () => { disposed = true; mounted.current = false; stop?.(); };
  }, [props.client]);
  let capture: CaptureSettingsPresentation | undefined;
  if (draft && saved && snapshot) {
    const devices = snapshot.microphones.map((device) => ({value:device.id ?? '',
      label:device.missing ? t('settings-device-missing') : device.id === null ? t('settings-device-default') : device.name,
      disabled:!device.available, detail:!device.available ? t('feedback-microphone-unavailable') : undefined}));
    if (draft.microphoneDeviceId && !devices.some((device) => device.value === draft.microphoneDeviceId)) {
      devices.push({value:draft.microphoneDeviceId, label:t('settings-device-missing'), disabled:true, detail:t('feedback-microphone-unavailable')});
    }
    capture = {
      draft, dirty, pending, unavailable:false, failure,
      change:(next) => { setDraft(next); setFailure(null); }, devices,
      audioSources:snapshot.audioSources.map((source) => ({value:source.source, disabled:!source.available})),
      chooseFolder:() => { submit({command:'choose-folder'}, false); },
      refreshDevices:() => { submit({command:'refresh-devices'}, false); },
      apply:() => {
        if (draft.videoMaxDurationSeconds === null || draft.audioBitrateKbps === null) { setFailure('web-settings-invalid'); return; }
        submit({command:'apply', expectedRevision:revision, draft}, 'profile');
      },
      cancel:() => { setDraft(structuredClone(saved)); setFailure(null); },
    };
  }
  const delivery: DeliverySettingsPresentation | undefined = snapshot ? {
    delivery:snapshot.delivery, draft:deliveryDraft, dirty:deliveryDirty, pending, unavailable:false, failure,
    change:(next) => { setDeliveryDraft(next); setFailure(null); },
    apply:() => { submit({command:'save-delivery', ...deliveryDraft}, 'delivery'); },
    cancel:() => { setDeliveryDraft({...deliverySaved}); setFailure(null); },
    refresh:() => { submit({command:'refresh-browsers'}, false); },
  } : undefined;
  return <><AppSettings {...props} preview={false} capture={capture} delivery={delivery} />
    {!capture && <div className="settings-load-state"><p role={failure ? 'alert' : 'status'}>{failure ? t('web-settings-load-failed') : t('web-settings-loading')}</p>
      {failure && <button type="button" className="tool-button" onClick={() => { submit({command:'open'}, 'all'); }}>{t('web-retry')}</button>}
    </div>}
    <dialog ref={dialog} className="dialog-surface" aria-labelledby="settings-discard-title" onCancel={() => setConfirmClose(false)}>
      <h2 id="settings-discard-title">{t('settings-discard')}</h2><p>{t('settings-dirty-description')}</p>
      <div className="dialog-actions"><button type="button" autoFocus className="tool-button" onClick={() => setConfirmClose(false)}>{t('settings-cancel')}</button>
        <button type="button" className="tool-button danger-button" onClick={close}>{t('settings-discard')}</button></div>
    </dialog>
  </>;
}
