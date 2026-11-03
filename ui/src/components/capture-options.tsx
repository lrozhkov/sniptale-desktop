import { Check, Monitor, Scan, Square } from 'lucide-react';
import { useId } from 'react';
import type { CaptureKind, CaptureSnapshot, CaptureTarget, CaptureQuality } from '../client/types';
import { useText } from '../localization/context';
import { Popover } from './menu';

const targetKeys = { region: 'panel-region', display: 'panel-screen', window: 'panel-window' };
const qualityKeys = { low: 'quality-low', balanced: 'quality-balanced', high: 'quality-high', ultra: 'quality-ultra' };
export function CaptureOptions({ kind, target, quality, setTarget, setQuality, snapshot, disabled }: {
  kind: CaptureKind; target: CaptureTarget; quality: CaptureQuality;
  setTarget: (value: CaptureTarget) => void; setQuality: (value: CaptureQuality) => void;
  snapshot: CaptureSnapshot | null; disabled: boolean;
}) {
  const { t } = useText();
  const group = useId();
  const SourceIcon = target === 'region' ? Scan : target === 'display' ? Monitor : Square;
  return <Popover label={t('web-capture-options')} disabled={disabled} content={() => <>
    <fieldset><legend>{t('panel-target-label')}</legend>
      {(['region', 'display', 'window'] as const).map((value) => {
        const availability = snapshot?.capabilities.targets[value];
        const Icon = value === 'region' ? Scan : value === 'display' ? Monitor : Square;
        return <label className="source-option" key={value}>
          <input type="radio" name={`${group}-source`} checked={target === value} value={value}
            disabled={!availability?.supported || !availability.sourceAvailable} onChange={() => setTarget(value)} />
          <Icon /><span>{t(targetKeys[value])}</span><Check className="choice-check" aria-hidden="true" />
        </label>;
      })}
    </fieldset>
    {kind === 'video' && <fieldset><legend>{t('web-recording-quality')}</legend>
      <div className="quality-options">{(['low', 'balanced', 'high', 'ultra'] as const).map((value) =>
        <label className="quality-option" key={value}>
          <input type="radio" name={`${group}-quality`} checked={quality === value} value={value} onChange={() => setQuality(value)} />
          <span>{t(qualityKeys[value])}</span>
        </label>)}
      </div><p className="parameter-hint">{t('web-quality-next-recording')}</p>
    </fieldset>}
  </>}>
    <SourceIcon /><span>{t(targetKeys[target])}</span>
    {kind === 'video' && <span className="quality-summary">{t(qualityKeys[quality])}</span>}
  </Popover>;
}
