import { FolderOpen, RefreshCw } from 'lucide-react';
import { useText } from '../localization/context';
import { DraftCheckbox, SettingsChoice, SettingsNumber, SettingsRow, SettingsSaveBar, ShortcutField } from './settings-controls';

export type CaptureSettingsSection = 'capture' | 'video' | 'audio' | 'shortcuts';
import type { CaptureSettingsDraft } from '../client/settings';
export type { CaptureSettingsDraft } from '../client/settings';
export type CaptureSettingsPresentation = {
  draft: CaptureSettingsDraft; change: (value: CaptureSettingsDraft) => void;
  devices: {value: string; label: string; disabled?: boolean; detail?: string}[];
  chooseFolder: () => void; refreshDevices: () => void; apply: () => void; cancel: () => void;
  audioSources?: {value: 'system' | 'microphone' | 'mixed'; disabled: boolean}[];
  dirty: boolean; pending: boolean; unavailable: boolean; failure: string | null;
};
const shortcuts = [
  ['screenshot-screen', 'shortcut-screen'], ['screenshot-active-window', 'shortcut-window'], ['screenshot-region', 'shortcut-region'],
  ['recording-start-screen', 'shortcut-record-screen'], ['recording-start-active-window', 'shortcut-record-window'],
  ['recording-start-region', 'shortcut-record-region'], ['recording-pause-resume', 'shortcut-pause-resume'], ['recording-stop', 'shortcut-stop'],
] as const;
export function CaptureSettingsForm({ section, model }: { section: CaptureSettingsSection; model: CaptureSettingsPresentation }) {
  const { t } = useText();
  const { draft, change } = model;
  const disabled = model.pending || model.unavailable;
  const patch = (next: Partial<CaptureSettingsDraft>) => change({ ...draft, ...next });
  return <form className="capture-settings-form" onSubmit={(event) => { event.preventDefault(); if (model.dirty && !disabled) model.apply(); }}>
    <fieldset disabled={disabled}>
    {section === 'capture' && <>
      <div className="settings-section-intro"><h2>{t('settings-save-location')}</h2><p>{t('settings-local-description')}</p></div>
      <div className="destination-field"><FolderOpen aria-hidden="true" /><span title={draft.destinationPath}><bdi dir="ltr">{draft.destinationPath || t('settings-choose-local-folder')}</bdi></span>
        <button type="button" className="tool-button" onClick={model.chooseFolder}>{t('settings-change-folder')}</button></div>
      <div className="settings-section-intro"><h2>{t('settings-screenshots')}</h2></div>
      <DraftCheckbox label={t('settings-screenshot-cursor')} checked={draft.screenshotIncludeCursor} onChange={(value) => patch({ screenshotIncludeCursor: value })} />
    </>}
    {section === 'video' && <>
      <SettingsRow label={t('settings-quality')}><SettingsChoice label={t('settings-quality')} value={draft.videoQuality}
        choices={(['low', 'balanced', 'high', 'ultra'] as const).map((value) => ({value, label:t(`quality-${value}`)}))} onChange={(videoQuality) => patch({videoQuality})} /></SettingsRow>
      <SettingsRow label={t('settings-frame-rate')}><SettingsChoice label={t('settings-frame-rate')} value={draft.videoFps}
        choices={(['24', '30', '60'] as const).map((value) => ({value, label:t('value-fps', {value})}))} onChange={(videoFps) => patch({videoFps})} /></SettingsRow>
      <SettingsRow label={t('settings-video-bitrate')} detail={t('web-bitrate-automatic')}><SettingsNumber label={t('settings-video-bitrate')} value={draft.videoBitrateOverrideMbps} optional unit="Mbps" onChange={(videoBitrateOverrideMbps) => patch({videoBitrateOverrideMbps})} /></SettingsRow>
      <SettingsRow label={t('settings-encoder')}><SettingsChoice label={t('settings-encoder')} value={draft.videoHardwarePreference}
        choices={[{value:'prefer', label:t('encoder-prefer')}, {value:'software', label:t('encoder-software')}]} onChange={(videoHardwarePreference) => patch({videoHardwarePreference})} /></SettingsRow>
      <SettingsRow label={t('settings-duration')}><SettingsNumber label={t('settings-duration')} unit={t('web-seconds-short')} value={draft.videoMaxDurationSeconds} onChange={(videoMaxDurationSeconds) => patch({videoMaxDurationSeconds})} /></SettingsRow>
      <DraftCheckbox label={t('field-include-cursor')} checked={draft.videoIncludeCursor} onChange={(videoIncludeCursor) => patch({videoIncludeCursor})} />
      <DraftCheckbox label={t('settings-pointer-highlights')} detail={t('web-pointer-highlights-detail')} checked={draft.collectPointerEvents} onChange={(collectPointerEvents) => patch({collectPointerEvents})} />
    </>}
    {section === 'audio' && <>
      <div className="settings-section-intro"><p>{t('settings-audio-description')}</p></div>
      <SettingsRow label={t('settings-audio-source')}><SettingsChoice label={t('settings-audio-source')} value={draft.audioSourceMode}
        choices={(model.audioSources ?? (['system', 'microphone', 'mixed'] as const).map((value) => ({value, disabled:false}))).map((option) => ({...option, label:t(`settings-audio-${option.value}`)}))} onChange={(audioSourceMode) => patch({audioSourceMode})} /></SettingsRow>
      <SettingsRow label={t('settings-microphone')}><div className="settings-inline-actions"><SettingsChoice label={t('settings-microphone')} value={draft.microphoneDeviceId}
        choices={model.devices} disabled={draft.audioSourceMode === 'system'} onChange={(microphoneDeviceId) => patch({microphoneDeviceId})} />
        <button type="button" className="tool-button" aria-label={t('settings-refresh-devices')} title={t('settings-refresh-devices')} onClick={model.refreshDevices}><RefreshCw /></button></div></SettingsRow>
      <SettingsRow label={t('settings-audio-quality')}><SettingsNumber label={t('settings-audio-quality')} unit="kbps" value={draft.audioBitrateKbps} onChange={(audioBitrateKbps) => patch({audioBitrateKbps})} /></SettingsRow>
    </>}
    {section === 'shortcuts' && <>
      <div className="settings-section-intro"><p>{t('settings-shortcuts-description')}</p></div>
      {shortcuts.map(([action, label]) => <SettingsRow key={action} label={t(label)}><ShortcutField label={t(label)} value={draft.hotkeys[action] ?? ''}
        onChange={(value) => patch({hotkeys:{...draft.hotkeys, [action]:value}})} /></SettingsRow>)}
    </>}
    </fieldset>
    <SettingsSaveBar {...model} />
  </form>;
}
