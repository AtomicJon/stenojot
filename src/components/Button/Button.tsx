import type { ReactNode } from 'react';
import s from './Button.module.scss';
import { ButtonSize, ButtonVariant } from './Button.const';

/** Props for the Button component. */
interface ButtonProps {
  /** Button contents. */
  children: ReactNode;
  /** Called when the button is clicked. */
  onClick: () => void;
  /** Visual style variant. */
  variant?: ButtonVariant;
  /** Size */
  size?: ButtonSize;
  /** Whether the button is disabled. */
  disabled?: boolean;
}

/**
 * The style class to use for each variant
 */
const VARIANT_STYLE_MAP: Record<ButtonVariant, string> = {
  [ButtonVariant.default]: s.default,
  [ButtonVariant.secondary]: s.secondary,
  [ButtonVariant.link]: s.link,
  [ButtonVariant.danger]: s.danger,
};

/**
 * The style class to use for each size
 */
const SIZE_STYLE_MAP: Record<ButtonSize, string> = {
  [ButtonSize.default]: s.medium,
  [ButtonSize.small]: s.small,
};

/** Styled button with accent, danger, and link variants. */
export function Button({
  children,
  onClick,
  variant = ButtonVariant.default,
  size = ButtonSize.default,
  disabled,
}: ButtonProps) {
  const variantClass = VARIANT_STYLE_MAP[variant];
  const sizeClass = SIZE_STYLE_MAP[size];

  return (
    <button
      className={`${s.button} ${variantClass} ${sizeClass}`}
      onClick={onClick}
      disabled={disabled}
    >
      {children}
    </button>
  );
}
