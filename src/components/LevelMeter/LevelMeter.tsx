import { getLevelPercent } from '../../lib/audio';
import s from './LevelMeter.module.scss';

/** Props for the LevelMeter component. */
interface LevelMeterProps {
  /** Display label (e.g. "Mic", "System"). */
  label: string;
  /** Current linear RMS value (0.0–1.0). */
  rms: number;
  /** Optional RMS threshold value to show as a marker on the bar. */
  thresholdRms?: number;
}

/** Audio level meter bar with optional detection threshold marker. */
export function LevelMeter({ label, rms, thresholdRms }: LevelMeterProps) {
  const pct = getLevelPercent(rms);
  const variant = pct < 50 ? s.low : pct < 80 ? s.mid : s.high;

  return (
    <div className={s.row}>
      <span className={s.label}>{label}</span>
      <div className={s.track}>
        <div className={`${s.fill} ${variant}`} style={{ width: `${pct}%` }} />
        {thresholdRms !== undefined && (
          <div
            className={s.threshold}
            style={{ left: `${getLevelPercent(thresholdRms)}%` }}
          />
        )}
      </div>
      <span className={s.value}>{Math.round(pct)}%</span>
    </div>
  );
}
