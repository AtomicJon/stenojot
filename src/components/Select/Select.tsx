import s from "./Select.module.scss";

/** A single option in the Select dropdown. */
interface SelectOption {
  value: string;
  label: string;
}

/** Props for the Select component. */
interface SelectProps {
  /** Display label shown above the dropdown. */
  label: string;
  /** Currently selected value. */
  value: string;
  /** Available options. */
  options: SelectOption[];
  /** Called when the selection changes. */
  onChange: (value: string) => void;
  /** Whether the select is disabled. */
  disabled?: boolean;
}

/** Labeled dropdown select input. */
export function Select({ label, value, options, onChange, disabled }: SelectProps) {
  return (
    <label className={s.wrapper}>
      <span className={s.label}>{label}</span>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        disabled={disabled}
        className={`${s.select} ${disabled ? s.disabled : ""}`}
      >
        {options.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
    </label>
  );
}
