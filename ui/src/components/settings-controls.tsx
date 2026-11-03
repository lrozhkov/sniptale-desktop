import { Check, Keyboard } from 'lucide-react';
import { useId, useState } from 'react';
import type { ReactNode } from 'react';
import { useText } from '../localization/context';
import { ChoiceMenu } from './menu';

export function SettingsRow({ label, detail, children }: { label: string; detail?: string; children: ReactNode }) {
  return <div className="settings-row"><div className="settings-row-copy"><span>{label}</span>{detail && <small>{detail}</small>}</div><div className="settings-row-control">{children}</div></div>;
}
export function DraftCheckbox({ label, detail, checked, disabled, onChange }: {
  label: string; detail?: string; checked: boolean; disabled?: boolean; onChange: (value: boolean) => void;
}) {
  const description = useId();
  return <label className="draft-checkbox"><input aria-label={label} aria-describedby={detail ? description : undefined} type="checkbox" checked={checked} disabled={disabled} onChange={(event) => onChange(event.target.checked)} />
    <span className="checkbox-mark" aria-hidden="true"><Check /></span><span>{label}{detail && <small id={description}>{detail}</small>}</span></label>;
}
export function SettingsChoice<T extends string>({ label, value, choices, onChange, disabled }: {
  label: string; value: T; choices: {value: T; label: string; disabled?: boolean; detail?: string}[];
  onChange: (value: T) => void; disabled?: boolean;
}) {
  return <ChoiceMenu appearance="field" label={label} value={value} choices={choices} disabled={disabled}
    onChange={(next) => { const selected = choices.find((choice) => choice.value === next); if (selected) onChange(selected.value); }}>
    <span title={choices.find((choice) => choice.value === value)?.label}>{choices.find((choice) => choice.value === value)?.label ?? value}</span>
  </ChoiceMenu>;
}
export function SettingsNumber({ label, value, onChange, unit, disabled, optional = false }: {
  label: string; value: number | null; onChange: (value: number | null) => void; unit: string; disabled?: boolean; optional?: boolean;
}) {
  const { t } = useText();
  return <label className="settings-number"><input aria-label={label} type="number" min="1" step="1" required={!optional}
    value={value ?? ''} disabled={disabled} placeholder={optional ? t('option-automatic') : undefined}
    onChange={(event) => onChange(Number.isFinite(event.target.valueAsNumber) ? event.target.valueAsNumber : null)} /><span>{unit}</span></label>;
}
export function ShortcutField({ label, value, disabled, onChange }: {
  label: string; value: string; disabled?: boolean; onChange: (value: string) => void;
}) {
  const { t } = useText();
  const id = useId();
  const [editing, setEditing] = useState(false);
  return <button type="button" className="shortcut-field" aria-label={label} aria-describedby={id}
    disabled={disabled} data-editing={editing} onClick={() => setEditing(true)} onBlur={() => setEditing(false)}
    onKeyDown={(event) => {
      if (!editing) return;
      if (event.key === 'Tab') { setEditing(false); return; }
      event.preventDefault(); event.stopPropagation();
      if (event.key === 'Escape') { setEditing(false); return; }
      if (['Control', 'Alt', 'Shift', 'Meta'].includes(event.key)) return;
      if ((event.key === 'Delete' || event.key === 'Backspace') && !event.ctrlKey && !event.altKey && !event.shiftKey && !event.metaKey) { onChange(''); setEditing(false); return; }
      const chord = [event.ctrlKey && 'Ctrl', event.altKey && 'Alt', event.shiftKey && 'Shift', event.metaKey && 'Win', event.code.replace(/^(Key|Digit)/, '')].filter(Boolean).join('+');
      onChange(chord); setEditing(false);
    }}><Keyboard aria-hidden="true" /><span id={id}>{editing ? t('shortcut-capturing') : value || t('shortcut-not-set')}</span></button>;
}

/** One action/status row for independently persisted settings forms. */
export function SettingsSaveBar({dirty, pending, unavailable, failure, cancel}: {
  dirty:boolean; pending:boolean; unavailable:boolean; failure:string | null; cancel:() => void;
}) {
  const { t } = useText();
  return <div className="settings-save-bar"><p role={failure ? 'alert' : 'status'}>{failure ? t(failure) : unavailable ? t('web-settings-unavailable') : pending ? t('web-settings-saving') : dirty ? t('web-settings-unsaved') : t('web-settings-up-to-date')}</p>
    <div className="settings-inline-actions"><button type="button" className="tool-button" disabled={!dirty || pending} onClick={cancel}>{t('settings-cancel')}</button>
      <button type="submit" className="primary-button" disabled={!dirty || pending || unavailable}>{t('settings-apply')}</button></div></div>;
}
