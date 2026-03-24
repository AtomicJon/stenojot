import s from './Slider.module.scss';

/** Props for the Slider component. */
interface SliderProps {
  /** Display label shown above the slider. */
  label: string;
  /** Current value. */
  value: number;
  /** Minimum allowed value. */
  min: number;
  /** Maximum allowed value. */
  max: number;
  /** Step increment. */
  step: number;
  /** Called when the slider value changes. */
  onChange: (value: number) => void;
}

/** Labeled range slider input. */
export function Slider({
  label,
  value,
  min,
  max,
  step,
  onChange,
}: SliderProps) {
  return (
    <label className={s.wrapper}>
      <span className={s.label}>{label}</span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(parseFloat(e.target.value))}
        className={s.slider}
      />
    </label>
  );
}
