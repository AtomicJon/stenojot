import type { ReactNode } from "react";
import s from "./Button.module.scss";

/** Props for the Button component. */
interface ButtonProps {
  /** Button contents. */
  children: ReactNode;
  /** Called when the button is clicked. */
  onClick: () => void;
  /** Visual style variant. */
  variant?: "accent" | "danger" | "link";
  /** Whether the button is disabled. */
  disabled?: boolean;
}

/** Styled button with accent, danger, and link variants. */
export function Button({
  children,
  onClick,
  variant = "accent",
  disabled,
}: ButtonProps) {
  const variantClass =
    variant === "danger"
      ? s.danger
      : variant === "link"
        ? s.link
        : s.accent;

  return (
    <button
      className={`${s.button} ${variantClass}`}
      onClick={onClick}
      disabled={disabled}
    >
      {children}
    </button>
  );
}
